//! Every user-visible feature is implemented as a [`Command`].
//!
//! The registry is built once in [`standard`] and shared by the tray menu and
//! the devices window.

mod exit;
mod open_data_folder;
mod open_log_file;
mod restore_all;
mod show_status;
mod show_window;
mod toggle_autostart;

pub mod ids;

pub use exit::Exit;
pub use open_data_folder::OpenDataFolder;
pub use open_log_file::OpenLogFile;
pub use restore_all::RestoreAll;
pub use show_status::ShowStatus;
pub use show_window::ShowWindow;
pub use toggle_autostart::ToggleAutostart;

use crate::core::command::CommandRegistry;

/// Registry with every command wired to its stable id.
pub fn standard() -> CommandRegistry {
    let mut registry = CommandRegistry::new();
    registry
        .register(ShowStatus)
        .register(ShowWindow)
        .register(RestoreAll)
        .register(OpenDataFolder)
        .register(OpenLogFile)
        .register(ToggleAutostart)
        .register(Exit);
    registry
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::command::CommandRegistry;

    fn has(registry: &CommandRegistry, id: &'static str) {
        assert!(registry.get(id).is_some(), "missing command {id}");
    }

    #[test]
    fn every_feature_is_registered() {
        let registry = standard();
        assert_eq!(registry.len(), 7);
        has(&registry, ids::SHOW_STATUS);
        has(&registry, ids::SHOW_WINDOW);
        has(&registry, ids::RESTORE_ALL);
        has(&registry, ids::OPEN_DATA_FOLDER);
        has(&registry, ids::OPEN_LOG_FILE);
        has(&registry, ids::TOGGLE_AUTOSTART);
        has(&registry, ids::EXIT);
    }

    #[test]
    fn ids_are_unique() {
        let registry = standard();
        let mut seen = std::collections::BTreeSet::new();
        for id in [
            ids::SHOW_STATUS,
            ids::SHOW_WINDOW,
            ids::RESTORE_ALL,
            ids::OPEN_DATA_FOLDER,
            ids::OPEN_LOG_FILE,
            ids::TOGGLE_AUTOSTART,
            ids::EXIT,
        ] {
            assert!(seen.insert(id));
            assert!(registry.get(id).unwrap().id() == id);
        }
    }
}
