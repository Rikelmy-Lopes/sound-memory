//! Tray context menu, built from the command registry.

use windows::core::PCWSTR;
use windows::Win32::Foundation::{HWND, LPARAM, POINT, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreatePopupMenu, DestroyMenu, GetCursorPos, PostMessageW, SetForegroundWindow,
    TrackPopupMenu, MF_CHECKED, MF_SEPARATOR, MF_STRING, MF_UNCHECKED, TPM_RETURNCMD,
    TPM_RIGHTBUTTON, WM_NULL,
};

use crate::commands::ids;
use crate::core::command::CommandRegistry;
use crate::core::util;

enum Entry {
    Item(&'static str),
    Separator,
}

const LAYOUT: &[Entry] = &[
    Entry::Item(ids::SHOW_STATUS),
    Entry::Item(ids::SHOW_WINDOW),
    Entry::Item(ids::RESTORE_ALL),
    Entry::Separator,
    Entry::Item(ids::OPEN_DATA_FOLDER),
    Entry::Item(ids::OPEN_LOG_FILE),
    Entry::Item(ids::TOGGLE_AUTOSTART),
    Entry::Separator,
    Entry::Item(ids::EXIT),
];

/// Shows the popup menu and returns the chosen command id, if any.
pub(super) fn show(hwnd: HWND, registry: &CommandRegistry) -> Option<&'static str> {
    unsafe {
        let menu = CreatePopupMenu().ok()?;
        let mut mapping: Vec<(&'static str, usize)> = Vec::new();
        let mut next_id = 1usize;

        for entry in LAYOUT {
            match entry {
                Entry::Separator => {
                    let _ = AppendMenuW(menu, MF_SEPARATOR, 0, PCWSTR::null());
                }
                Entry::Item(id) => {
                    let Some(command) = registry.get(id) else {
                        continue;
                    };
                    let flags = if command.checked() {
                        MF_STRING | MF_CHECKED
                    } else {
                        MF_STRING | MF_UNCHECKED
                    };
                    let label = util::to_wide(command.label());
                    let _ = AppendMenuW(
                        menu,
                        flags,
                        next_id,
                        PCWSTR(label.as_ptr()),
                    );
                    mapping.push((*id, next_id));
                    next_id += 1;
                }
            }
        }

        let mut point = POINT::default();
        let _ = GetCursorPos(&mut point);
        let _ = SetForegroundWindow(hwnd);

        let chosen = TrackPopupMenu(
            menu,
            TPM_RIGHTBUTTON | TPM_RETURNCMD,
            point.x,
            point.y,
            Some(0),
            hwnd,
            None,
        );

        let _ = PostMessageW(Some(hwnd), WM_NULL, WPARAM(0), LPARAM(0));
        let _ = DestroyMenu(menu);

        mapping
            .into_iter()
            .find(|(_, menu_id)| *menu_id == chosen.0 as usize)
            .map(|(id, _)| id)
    }
}
