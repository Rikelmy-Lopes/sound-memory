//! Minimal registry access for the per-user autostart entry.

use std::io;

use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS};
use windows::Win32::System::Registry::{
    RegCloseKey, RegDeleteValueW, RegOpenKeyExW, RegQueryValueExW, RegSetValueExW, HKEY,
    HKEY_CURRENT_USER, KEY_QUERY_VALUE, KEY_SET_VALUE, REG_SZ,
};

use crate::core::util;

const RUN_KEY: PCWSTR = w!("Software\\Microsoft\\Windows\\CurrentVersion\\Run");
const VALUE_NAME: PCWSTR = w!("BtVolume");

pub fn is_enabled() -> bool {
    unsafe {
        let mut key = HKEY::default();
        if RegOpenKeyExW(HKEY_CURRENT_USER, RUN_KEY, None, KEY_QUERY_VALUE, &mut key)
            != ERROR_SUCCESS
        {
            return false;
        }

        let mut size = 0u32;
        let status = RegQueryValueExW(key, VALUE_NAME, None, None, None, Some(&mut size));
        let _ = RegCloseKey(key);

        status == ERROR_SUCCESS && size > 0
    }
}

pub fn set_enabled(enabled: bool) -> io::Result<()> {
    unsafe {
        let mut key = HKEY::default();
        let status = RegOpenKeyExW(HKEY_CURRENT_USER, RUN_KEY, None, KEY_SET_VALUE, &mut key);
        if status != ERROR_SUCCESS {
            return Err(os_error(status.0));
        }

        let result = if enabled {
            write_command(key)
        } else {
            remove_value(key)
        };
        let _ = RegCloseKey(key);
        result
    }
}

unsafe fn write_command(key: HKEY) -> io::Result<()> {
    let exe = std::env::current_exe()?;
    let command = format!("\"{}\"", exe.display());
    let wide = util::to_wide(&command);
    let bytes = unsafe {
        std::slice::from_raw_parts(
            wide.as_ptr() as *const u8,
            wide.len() * std::mem::size_of::<u16>(),
        )
    };

    let status = unsafe { RegSetValueExW(key, VALUE_NAME, None, REG_SZ, Some(bytes)) };
    if status == ERROR_SUCCESS {
        Ok(())
    } else {
        Err(os_error(status.0))
    }
}

unsafe fn remove_value(key: HKEY) -> io::Result<()> {
    let status = unsafe { RegDeleteValueW(key, VALUE_NAME) };
    if status == ERROR_SUCCESS || status == ERROR_FILE_NOT_FOUND {
        Ok(())
    } else {
        Err(os_error(status.0))
    }
}

fn os_error(code: u32) -> io::Error {
    io::Error::from_raw_os_error(code as i32)
}
