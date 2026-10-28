//! Audio engine loop: watches Bluetooth render endpoints, remembers their
//! volume and restores it whenever the same device reconnects.
//!
//! All COM work happens on one MTA worker thread.  Notification callbacks only
//! post small messages into a channel, which keeps re-entrancy out of the COM
//! callbacks.

use std::collections::{HashMap, HashSet};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender};
use std::sync::Arc;
use std::time::{Duration, Instant};

use windows::core::{GUID, IUnknown};
use windows::Win32::Media::Audio::Endpoints::IAudioEndpointVolume;
use windows::Win32::Media::Audio::Endpoints::IAudioEndpointVolumeCallback;
use windows::Win32::Media::Audio::{IMMDeviceEnumerator, IMMNotificationClient, MMDeviceEnumerator};
use windows::Win32::System::Com::{CoCreateInstance, CLSCTX_ALL};

use super::com::Com;
use super::endpoint::{enumerate_endpoints, Endpoint};
use super::notifier::{DeviceNotifier, Event, VolumeNotifier};
use super::policy::{
    next_retry_delay, outside_grace, BT_VOLUME_CONTEXT, LOOP_TICK, SCAN_INTERVAL,
};
use super::request::AudioRequest;
use crate::core::state::{DeviceStatus, Shared};
use crate::core::util;
use crate::features::store::{self, Store};

struct Tracked {
    name: String,
    level: f32,
    volume: IAudioEndpointVolume,
    callback: IAudioEndpointVolumeCallback,
    connected_at: Instant,
    /// Last time an external (not ours) volume notification was observed.
    changed_at: Option<Instant>,
}

struct PendingRestore {
    id: String,
    units: u32,
    target: f32,
    attempts: u32,
    next: Instant,
}

/// Entry point for the audio worker thread.
pub fn run(shared: Arc<Shared>, requests: Receiver<AudioRequest>) {
    let _com = match Com::init() {
        Ok(com) => com,
        Err(err) => {
            shared.note(format!("Audio engine could not initialize COM: {err}"));
            return;
        }
    };

    let mut engine = match Engine::new(shared.clone()) {
        Ok(engine) => engine,
        Err(err) => {
            shared.note(format!("Audio engine could not start: {err}"));
            return;
        }
    };

    engine.run(requests);
}

struct Engine {
    shared: Arc<Shared>,
    enumerator: IMMDeviceEnumerator,
    notifier: IMMNotificationClient,
    events: Receiver<Event>,
    events_tx: Sender<Event>,
    tracked: HashMap<String, Tracked>,
    pending: Vec<PendingRestore>,
    last_scan: Instant,
    last_flush: Instant,
}

impl Engine {
    fn new(shared: Arc<Shared>) -> windows::core::Result<Self> {
        let enumerator: IMMDeviceEnumerator =
            unsafe { CoCreateInstance(&MMDeviceEnumerator, None::<&IUnknown>, CLSCTX_ALL) }?;

        let (events_tx, events) = std::sync::mpsc::channel();
        let notifier: IMMNotificationClient = DeviceNotifier {
            tx: events_tx.clone(),
        }
        .into();

        unsafe { enumerator.RegisterEndpointNotificationCallback(&notifier)? };

        Ok(Self {
            shared,
            enumerator,
            notifier,
            events,
            events_tx,
            tracked: HashMap::new(),
            pending: Vec::new(),
            last_scan: Instant::now(),
            last_flush: Instant::now(),
        })
    }

    fn run(&mut self, requests: Receiver<AudioRequest>) {
        util::log("audio engine started");
        self.scan();

        loop {
            let now = Instant::now();
            self.apply_due_restores(now);
            if now.duration_since(self.last_scan) >= SCAN_INTERVAL {
                self.scan();
            }
            self.flush_if_due(false);

            let timeout = self
                .pending
                .iter()
                .map(|pending| pending.next.saturating_duration_since(now))
                .min()
                .unwrap_or(LOOP_TICK)
                .clamp(Duration::from_millis(10), LOOP_TICK);

            let request = requests.recv_timeout(timeout);

            // Drain callback events even when a request arrived.
            let events: Vec<Event> = self.events.try_iter().collect();
            for event in events {
                match event {
                    Event::Rescan => self.scan(),
                    Event::Volume { id, level, context } => self.on_volume(&id, level, context),
                }
            }

            match request {
                Ok(AudioRequest::Shutdown) => break,
                Ok(AudioRequest::RestoreAll) => self.restore_all(),
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => break,
            }
        }

        self.shutdown();
    }

    fn scan(&mut self) {
        self.last_scan = Instant::now();

        let endpoints = match unsafe { enumerate_endpoints(&self.enumerator) } {
            Ok(endpoints) => endpoints,
            Err(err) => {
                util::log(&format!("could not enumerate audio endpoints: {err}"));
                return;
            }
        };

        let now = Instant::now();
        let mut seen = HashSet::new();

        for endpoint in endpoints {
            seen.insert(endpoint.id.clone());
            if let Some(tracked) = self.tracked.get_mut(&endpoint.id) {
                tracked.level = endpoint.level;
                tracked.name = endpoint.name;
            } else {
                self.add_device(endpoint, now);
            }
        }

        let gone: Vec<String> = self
            .tracked
            .keys()
            .filter(|id| !seen.contains(*id))
            .cloned()
            .collect();
        for id in gone {
            self.drop_device(&id);
        }

        self.publish_status();
    }

    fn add_device(&mut self, endpoint: Endpoint, now: Instant) {
        let callback: IAudioEndpointVolumeCallback = VolumeNotifier {
            id: endpoint.id.clone(),
            tx: self.events_tx.clone(),
        }
        .into();

        if let Err(err) = unsafe { endpoint.volume.RegisterControlChangeNotify(&callback) } {
            util::log(&format!(
                "could not watch volume changes for '{}': {err}",
                endpoint.name
            ));
        }

        let stored = self.stored_level(&endpoint.id, &endpoint.name);
        let mut tracked = Tracked {
            name: endpoint.name.clone(),
            level: endpoint.level,
            volume: endpoint.volume,
            callback,
            connected_at: now,
            changed_at: None,
        };

        match stored {
            Some(units) => {
                tracked.level = store::scalar_from_units(units);
                self.pending.retain(|pending| pending.id != endpoint.id);
                self.pending.push(PendingRestore {
                    id: endpoint.id.clone(),
                    units,
                    target: store::scalar_from_units(units),
                    attempts: 0,
                    next: now,
                });
                self.shared.note(format!(
                    "{} connected · restoring volume to {}%",
                    endpoint.name,
                    store::percent(units)
                ));
            }
            None => {
                let units = store::units_from_scalar(endpoint.level);
                {
                    let mut store = self.lock_store();
                    if store.set(&endpoint.id, &endpoint.name, units) {
                        let _ = store.flush();
                    }
                }
                self.shared.note(format!(
                    "{} connected · saved current volume ({}%)",
                    endpoint.name,
                    store::percent(units)
                ));
            }
        }

        util::log(&format!(
            "tracking Bluetooth device '{}' ({})",
            endpoint.name, endpoint.id
        ));
        self.tracked.insert(endpoint.id, tracked);
    }

    fn drop_device(&mut self, id: &str) {
        let Some(tracked) = self.tracked.remove(id) else {
            return;
        };

        unsafe {
            let _ = tracked
                .volume
                .UnregisterControlChangeNotify(&tracked.callback);
        }

        // Changes made within the connect grace period are usually the headset's
        // own volume sync, so they must not overwrite the remembered level.
        if let Some(changed) = tracked.changed_at {
            if outside_grace(tracked.connected_at, changed) {
                let units = store::units_from_scalar(tracked.level);
                let mut store = self.lock_store();
                store.set(id, &tracked.name, units);
                let _ = store.flush();
            }
        }

        self.pending.retain(|pending| pending.id != id);
        util::log(&format!("Bluetooth device '{}' disconnected", tracked.name));
    }

    fn on_volume(&mut self, id: &str, level: f32, context: GUID) {
        if context == BT_VOLUME_CONTEXT {
            return; // our own restore
        }

        let Some(tracked) = self.tracked.get_mut(id) else {
            return;
        };

        let now = Instant::now();
        tracked.level = level.clamp(0.0, 1.0);
        tracked.changed_at = Some(now);

        if !outside_grace(tracked.connected_at, now) {
            return;
        }

        let units = store::units_from_scalar(tracked.level);
        let name = tracked.name.clone();
        let mut store = self.lock_store();
        if store.set(id, &name, units) {
            util::log(&format!(
                "saved volume {}% for '{}'",
                store::percent(units),
                name
            ));
        }
    }

    fn apply_due_restores(&mut self, now: Instant) {
        if self.pending.is_empty() {
            return;
        }

        let mut remaining = Vec::new();
        let mut notes = Vec::new();
        let due = std::mem::take(&mut self.pending);

        for mut pending in due {
            if pending.next > now {
                remaining.push(pending);
                continue;
            }

            let Some(tracked) = self.tracked.get_mut(&pending.id) else {
                continue; // disconnected while waiting
            };

            match unsafe {
                tracked
                    .volume
                    .SetMasterVolumeLevelScalar(pending.target, &BT_VOLUME_CONTEXT)
            } {
                Ok(()) => {
                    tracked.level = pending.target;
                    if pending.attempts == 0 {
                        util::log(&format!(
                            "restoring '{}' to {}%",
                            tracked.name,
                            store::percent(pending.units)
                        ));
                        notes.push(format!(
                            "{} · volume restored to {}%",
                            tracked.name,
                            store::percent(pending.units)
                        ));
                    }
                    pending.attempts += 1;

                    match next_retry_delay(pending.attempts) {
                        Some(delay) => {
                            pending.next = now + delay;
                            remaining.push(pending);
                        }
                        None => {
                            let actual = unsafe { tracked.volume.GetMasterVolumeLevelScalar() }
                                .unwrap_or(pending.target);
                            if (actual - pending.target).abs() > 0.01 {
                                util::log(&format!(
                                    "warning: '{}' volume is {:.0}% after restore (wanted {:.0}%)",
                                    tracked.name,
                                    actual * 100.0,
                                    pending.target * 100.0
                                ));
                            }
                        }
                    }
                }
                Err(err) => {
                    util::log(&format!(
                        "could not restore volume for '{}': {err}",
                        tracked.name
                    ));
                    notes.push(format!(
                        "{} · could not set the volume (is the device still connected?)",
                        tracked.name
                    ));
                }
            }
        }

        self.pending = remaining;
        for note in notes {
            self.shared.note(note);
        }
        self.publish_status();
    }

    fn restore_all(&mut self) {
        let devices: Vec<(String, String)> = self
            .tracked
            .iter()
            .map(|(id, tracked)| (id.clone(), tracked.name.clone()))
            .collect();

        let mut restored = 0;
        for (id, name) in devices {
            if let Some(units) = self.stored_level(&id, &name) {
                self.pending.retain(|pending| pending.id != id);
                self.pending.push(PendingRestore {
                    id,
                    units,
                    target: store::scalar_from_units(units),
                    attempts: 0,
                    next: Instant::now(),
                });
                restored += 1;
            }
        }

        if restored == 0 {
            self.shared
                .note("No saved volume for the connected Bluetooth device(s) yet.");
        } else {
            util::log(&format!(
                "manual restore requested for {restored} device(s)"
            ));
        }
    }

    fn stored_level(&self, id: &str, name: &str) -> Option<u32> {
        let mut store = self.lock_store();
        let found = store.lookup(id, name);
        if let Some((units, Some(old_id))) = &found {
            store.adopt(old_id, id, name, *units);
            util::log(&format!(
                "moved saved volume from old endpoint id {old_id} to {id}"
            ));
        }
        found.map(|(units, _)| units)
    }

    fn lock_store(&self) -> std::sync::MutexGuard<'_, Store> {
        self.shared
            .store
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn flush_if_due(&mut self, force: bool) {
        if !force && self.last_flush.elapsed() < Duration::from_secs(2) {
            return;
        }
        if let Err(err) = self.lock_store().flush() {
            util::log(&format!("could not save volume store: {err}"));
        }
        self.last_flush = Instant::now();
    }

    fn publish_status(&self) {
        let mut devices: Vec<DeviceStatus> = self
            .tracked
            .values()
            .map(|tracked| DeviceStatus {
                name: tracked.name.clone(),
                level_pct: (tracked.level.clamp(0.0, 1.0) * 100.0).round() as u32,
            })
            .collect();
        devices.sort_by(|a, b| a.name.cmp(&b.name));

        if let Ok(mut current) = self.shared.devices.lock() {
            *current = devices;
        }
    }

    fn shutdown(&mut self) {
        util::log("audio engine stopping");

        self.pending.clear();
        unsafe {
            let _ = self
                .enumerator
                .UnregisterEndpointNotificationCallback(&self.notifier);
        }
        for (_, tracked) in self.tracked.drain() {
            unsafe {
                let _ = tracked
                    .volume
                    .UnregisterControlChangeNotify(&tracked.callback);
            }
        }

        self.flush_if_due(true);
        if let Ok(mut devices) = self.shared.devices.lock() {
            devices.clear();
        }
    }
}
