//! Opens files and folders with the shell's default handler.

use std::path::Path;

use windows::core::{w, PCWSTR};
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

use super::wide::to_wide;

pub fn open_path(path: &Path) {
    let path = to_wide(&path.to_string_lossy());
    unsafe {
        let _ = ShellExecuteW(
            None,
            w!("open"),
            PCWSTR(path.as_ptr()),
            PCWSTR::null(),
            PCWSTR::null(),
            SW_SHOWNORMAL,
        );
    }
}
