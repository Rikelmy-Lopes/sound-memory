//! COM callbacks that translate Windows notifications into plain channel events.
//!
//! Callbacks must stay short and non-reentrant: they only post an [`Event`].

use std::sync::mpsc::Sender;

use windows::core::{implement, GUID, PCWSTR};
use windows::Win32::Foundation::PROPERTYKEY;
use windows::Win32::Media::Audio::Endpoints::{
    IAudioEndpointVolumeCallback, IAudioEndpointVolumeCallback_Impl,
};
use windows::Win32::Media::Audio::{
    EDataFlow, ERole, IMMNotificationClient, IMMNotificationClient_Impl,
    AUDIO_VOLUME_NOTIFICATION_DATA, DEVICE_STATE,
};

/// Events produced by the COM callbacks.
pub(crate) enum Event {
    Rescan,
    Volume {
        id: String,
        level: f32,
        context: GUID,
    },
}

#[implement(IMMNotificationClient)]
pub(crate) struct DeviceNotifier {
    pub tx: Sender<Event>,
}

impl DeviceNotifier {
    fn ping(&self) {
        let _ = self.tx.send(Event::Rescan);
    }
}

impl IMMNotificationClient_Impl for DeviceNotifier_Impl {
    fn OnDeviceStateChanged(&self, _: &PCWSTR, _: DEVICE_STATE) -> windows::core::Result<()> {
        self.ping();
        Ok(())
    }

    fn OnDeviceAdded(&self, _: &PCWSTR) -> windows::core::Result<()> {
        self.ping();
        Ok(())
    }

    fn OnDeviceRemoved(&self, _: &PCWSTR) -> windows::core::Result<()> {
        self.ping();
        Ok(())
    }

    fn OnDefaultDeviceChanged(
        &self,
        _: EDataFlow,
        _: ERole,
        _: &PCWSTR,
    ) -> windows::core::Result<()> {
        self.ping();
        Ok(())
    }

    fn OnPropertyValueChanged(&self, _: &PCWSTR, _: &PROPERTYKEY) -> windows::core::Result<()> {
        Ok(())
    }
}

#[implement(IAudioEndpointVolumeCallback)]
pub(crate) struct VolumeNotifier {
    pub id: String,
    pub tx: Sender<Event>,
}

impl IAudioEndpointVolumeCallback_Impl for VolumeNotifier_Impl {
    fn OnNotify(&self, pnotify: *mut AUDIO_VOLUME_NOTIFICATION_DATA) -> windows::core::Result<()> {
        if !pnotify.is_null() {
            let data = unsafe { &*pnotify };
            let _ = self.tx.send(Event::Volume {
                id: self.id.clone(),
                level: data.fMasterVolume,
                context: data.guidEventContext,
            });
        }
        Ok(())
    }
}
