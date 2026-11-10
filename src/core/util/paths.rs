//! Application data locations.

use std::path::{Path, PathBuf};

pub fn data_dir() -> PathBuf {
    match std::env::var_os("LOCALAPPDATA") {
        Some(dir) => PathBuf::from(dir).join("Sound Memory"),
        None => std::env::temp_dir().join("Sound Memory"),
    }
}

pub fn log_path() -> PathBuf {
    data_dir().join("Sound Memory.log")
}

pub fn ensure_data_dir() -> std::io::Result<PathBuf> {
    let dir = data_dir();
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

pub fn ensure_parent(path: &Path) {
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
}

/// Copies the store and log from the old `sound-memory` folder, if the new
/// folder does not have them yet.  The old store is tab separated, which
/// [`crate::features::store::Store::load`] still understands, so it is written
/// as JSON on the next change.
pub fn migrate_legacy_data(dir: &Path) {
    let Some(local) = std::env::var_os("LOCALAPPDATA") else {
        return;
    };
    migrate_from(&PathBuf::from(local).join("sound-memory"), dir);
}

fn migrate_from(legacy: &Path, dir: &Path) {
    let copies = [
        ("devices.tsv", "devices.json"),
        ("sound-memory.log", "Sound Memory.log"),
    ];

    for (old_name, new_name) in copies {
        let old = legacy.join(old_name);
        let new = dir.join(new_name);
        if old.is_file() && !new.exists() {
            let _ = std::fs::copy(old, new);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "sound-memory-paths-{}-{tag}-{nanos}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn copies_legacy_store_and_log_without_overwriting() {
        let root = temp_dir("migrate");
        let legacy = root.join("sound-memory");
        let target = root.join("Sound Memory");
        std::fs::create_dir_all(&legacy).unwrap();
        std::fs::create_dir_all(&target).unwrap();
        std::fs::write(legacy.join("devices.tsv"), "id\t500\tHeadset\n").unwrap();
        std::fs::write(legacy.join("sound-memory.log"), "old log\n").unwrap();

        migrate_from(&legacy, &target);

        let store = std::fs::read_to_string(target.join("devices.json")).unwrap();
        assert_eq!(store, "id\t500\tHeadset\n");
        let log = std::fs::read_to_string(target.join("Sound Memory.log")).unwrap();
        assert_eq!(log, "old log\n");

        // Existing files in the new folder are never replaced.
        std::fs::write(target.join("devices.json"), "keep").unwrap();
        migrate_from(&legacy, &target);
        assert_eq!(
            std::fs::read_to_string(target.join("devices.json")).unwrap(),
            "keep"
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn migration_is_silent_when_legacy_folder_is_missing() {
        let root = temp_dir("missing");
        let target = root.join("Sound Memory");
        std::fs::create_dir_all(&target).unwrap();

        migrate_from(&root.join("sound-memory"), &target);

        assert!(!target.join("devices.json").exists());
        assert!(!target.join("Sound Memory.log").exists());
        std::fs::remove_dir_all(root).unwrap();
    }
}
