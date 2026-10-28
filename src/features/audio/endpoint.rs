//! Core Audio endpoint discovery and property reading.

use windows::core::GUID;
use windows::Win32::Foundation::PROPERTYKEY;
use windows::Win32::Media::Audio::Endpoints::IAudioEndpointVolume;
use windows::Win32::Media::Audio::{
    eRender, IMMDevice, IMMDeviceEnumerator, DEVICE_STATE_ACTIVE,
};
use windows::Win32::System::Com::StructuredStorage::{PropVariantClear, PROPVARIANT};
use windows::Win32::System::Com::{CoTaskMemFree, CLSCTX_ALL, STGM_READ};
use windows::Win32::System::Variant::VT_LPWSTR;
use windows::Win32::UI::Shell::PropertiesSystem::IPropertyStore;

use super::policy::is_bluetooth;
use crate::core::util;

pub(crate) struct Endpoint {
    pub id: String,
    pub name: String,
    pub level: f32,
    pub volume: IAudioEndpointVolume,
}

// Device properties.  Kept locally so no extra feature/module is required.
const PKEY_DEVICE_FRIENDLY_NAME: PROPERTYKEY = PROPERTYKEY {
    fmtid: GUID::from_u128(0xa45c254e_df1c_4efd_8020_67d146a850e0),
    pid: 14,
};
const PKEY_DEVICE_ENUMERATOR_NAME: PROPERTYKEY = PROPERTYKEY {
    fmtid: GUID::from_u128(0xa45c254e_df1c_4efd_8020_67d146a850e0),
    pid: 24,
};
const PKEY_DEVICE_INSTANCE_ID: PROPERTYKEY = PROPERTYKEY {
    fmtid: GUID::from_u128(0x78c34fc8_104a_4aca_9ea4_524d52996e57),
    pid: 256,
};

pub(crate) unsafe fn enumerate_endpoints(
    enumerator: &IMMDeviceEnumerator,
) -> windows::core::Result<Vec<Endpoint>> {
    let collection = enumerator.EnumAudioEndpoints(eRender, DEVICE_STATE_ACTIVE)?;
    let count = collection.GetCount()?;
    let mut endpoints = Vec::with_capacity(count as usize);

    for index in 0..count {
        let device = match collection.Item(index) {
            Ok(device) => device,
            Err(err) => {
                util::log(&format!("could not open audio endpoint {index}: {err}"));
                continue;
            }
        };

        match endpoint_from_device(device) {
            Ok(Some(endpoint)) => endpoints.push(endpoint),
            Ok(None) => {}
            Err(err) => util::log(&format!("could not inspect audio endpoint {index}: {err}")),
        }
    }

    Ok(endpoints)
}

unsafe fn endpoint_from_device(device: IMMDevice) -> windows::core::Result<Option<Endpoint>> {
    let id_ptr = device.GetId()?;
    let id = if id_ptr.is_null() {
        String::new()
    } else {
        id_ptr.to_string().unwrap_or_default()
    };
    if !id_ptr.is_null() {
        CoTaskMemFree(Some(id_ptr.0 as *const std::ffi::c_void));
    }
    if id.is_empty() {
        return Ok(None);
    }

    let props = match device.OpenPropertyStore(STGM_READ) {
        Ok(props) => props,
        Err(err) => {
            util::log(&format!(
                "could not read properties of endpoint {id}: {err}"
            ));
            return Ok(None);
        }
    };

    let instance = get_string_prop(&props, &PKEY_DEVICE_INSTANCE_ID).unwrap_or_default();
    let enumerator_name = get_string_prop(&props, &PKEY_DEVICE_ENUMERATOR_NAME).unwrap_or_default();
    if !is_bluetooth(&instance, &enumerator_name) {
        return Ok(None);
    }

    let name = get_string_prop(&props, &PKEY_DEVICE_FRIENDLY_NAME)
        .map(|name| name.trim().to_string())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "Bluetooth audio device".to_string());

    let volume: IAudioEndpointVolume = device.Activate(CLSCTX_ALL, None)?;
    let level = volume.GetMasterVolumeLevelScalar()?;

    Ok(Some(Endpoint {
        id,
        name,
        level,
        volume,
    }))
}

unsafe fn get_string_prop(props: &IPropertyStore, key: &PROPERTYKEY) -> Option<String> {
    let mut value: PROPVARIANT = props.GetValue(key).ok()?;
    let string = prop_string(&value);
    let _ = PropVariantClear(&mut value);
    string
}

unsafe fn prop_string(value: &PROPVARIANT) -> Option<String> {
    let inner = &value.Anonymous.Anonymous;
    if inner.vt != VT_LPWSTR {
        return None;
    }
    let text = inner.Anonymous.pwszVal;
    if text.is_null() {
        None
    } else {
        text.to_string().ok()
    }
}
