//! Data shared between the tray/UI threads and the audio engine thread.

use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::Mutex;

use crate::core::util;
use crate::features::store::Store;

#[derive(Clone, Debug)]
pub struct DeviceStatus {
    pub name: String,
    pub level_pct: u32,
}

/// A record loaded from the persistent store, as shown in the UI window.
#[derive(Clone, Debug)]
pub struct SavedDevice {
    pub id: String,
    pub name: String,
    pub units: u32,
}

pub struct Shared {
    pub store: Mutex<Store>,
    pub devices: Mutex<Vec<DeviceStatus>>,
    pub notes: Mutex<VecDeque<String>>,
    pub data_dir: PathBuf,
    pub log_file: PathBuf,
}

impl Shared {
    pub fn new(store: Store, data_dir: PathBuf, log_file: PathBuf) -> Self {
        Self {
            store: Mutex::new(store),
            devices: Mutex::new(Vec::new()),
            notes: Mutex::new(VecDeque::new()),
            data_dir,
            log_file,
        }
    }

    /// Writes an event to the log and queues a notification for the tray.
    pub fn note(&self, text: impl Into<String>) {
        let text = text.into();
        util::log(&text);

        let mut notes = self
            .notes
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if notes.len() < 32 {
            notes.push_back(text);
        }
    }

    /// Snapshot of every device remembered on disk.
    pub fn saved_devices(&self) -> Vec<SavedDevice> {
        let store = self
            .store
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        store
            .list()
            .map(|(id, entry)| SavedDevice {
                id: id.to_string(),
                name: entry.name.clone(),
                units: entry.level,
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::store::Store;

    fn shared() -> Shared {
        Shared::new(
            Store::load(std::env::temp_dir().join("sound-memory-state-test.tsv")),
            std::env::temp_dir(),
            std::env::temp_dir().join("sound-memory-state-test.log"),
        )
    }

    #[test]
    fn saved_devices_are_copied_out_of_the_store() {
        let shared = shared();
        {
            let mut store = shared.store.lock().unwrap();
            store.set("id-1", "Headset", 4200);
            store.set("id-2", "Speaker", 8000);
        }

        let devices = shared.saved_devices();
        assert_eq!(devices.len(), 2);
        assert_eq!(devices[0].id, "id-1");
        assert_eq!(devices[1].units, 8000);
        assert_eq!(devices[1].name, "Speaker");
    }

    #[test]
    fn notes_queue_is_bounded() {
        let shared = shared();
        for index in 0..40 {
            shared.note(format!("note {index}"));
        }

        let notes = shared.notes.lock().unwrap();
        assert_eq!(notes.len(), 32);
        assert_eq!(notes.front().unwrap(), "note 0");
        assert_eq!(notes.back().unwrap(), "note 31");
    }
}
