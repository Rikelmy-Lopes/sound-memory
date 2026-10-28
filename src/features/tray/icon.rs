//! Notification-area icon primitives: creation, tooltip, balloons and the
//! [`Notifier`] adapter used by commands.

use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Shell::{
    Shell_NotifyIconW, NIF_ICON, NIF_INFO, NIF_MESSAGE, NIF_TIP, NIIF_INFO, NIIF_NOSOUND,
    NIIF_WARNING, NIM_ADD, NIM_MODIFY, NOTIFYICONDATAW,
};
use windows::Win32::UI::WindowsAndMessaging::HICON;

use crate::core::command::Notifier;
use crate::core::util;

pub(super) const TITLE: &str = "sound-memory";
pub(super) const CALLBACK_MESSAGE: u32 =
    windows::Win32::UI::WindowsAndMessaging::WM_APP + 1;

pub(super) fn make_icon_data(hwnd: HWND, icon: HICON) -> NOTIFYICONDATAW {
    let mut nid = NOTIFYICONDATAW {
        cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: hwnd,
        uID: 1,
        uFlags: NIF_ICON | NIF_MESSAGE | NIF_TIP,
        uCallbackMessage: CALLBACK_MESSAGE,
        hIcon: icon,
        ..Default::default()
    };
    util::set_wide(&mut nid.szTip, TITLE);
    nid
}

pub(super) fn add(nid: &NOTIFYICONDATAW) -> bool {
    for attempt in 0..10 {
        if unsafe { Shell_NotifyIconW(NIM_ADD, nid) }.as_bool() {
            return true;
        }
        if attempt < 9 {
            std::thread::sleep(std::time::Duration::from_millis(500));
        }
    }
    false
}

pub(super) fn update_tip(base: &NOTIFYICONDATAW, tip: &str) {
    let mut nid = *base;
    nid.uFlags = NIF_TIP;
    util::set_wide(&mut nid.szTip, tip);
    unsafe {
        let _ = Shell_NotifyIconW(NIM_MODIFY, &nid);
    }
}

pub(super) fn balloon(base: &NOTIFYICONDATAW, text: &str, warning: bool) {
    let mut nid = *base;
    nid.uFlags = NIF_INFO;
    nid.dwInfoFlags = if warning {
        NIIF_WARNING | NIIF_NOSOUND
    } else {
        NIIF_INFO | NIIF_NOSOUND
    };
    util::set_wide(&mut nid.szInfoTitle, TITLE);
    util::set_wide(&mut nid.szInfo, &util::truncate(text, 240));
    unsafe {
        let _ = Shell_NotifyIconW(NIM_MODIFY, &nid);
    }
}

/// Command-side view of the tray icon, so commands can show balloons without
/// knowing anything about Win32.
#[derive(Clone)]
pub(super) struct TrayNotifier {
    nid: NOTIFYICONDATAW,
}

impl TrayNotifier {
    pub(super) fn new(nid: NOTIFYICONDATAW) -> Self {
        Self { nid }
    }
}

impl Notifier for TrayNotifier {
    fn notify(&self, text: &str, warning: bool) {
        balloon(&self.nid, text, warning);
    }
}
