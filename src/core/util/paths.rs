//! Application data locations.

use std::path::{Path, PathBuf};

pub fn data_dir() -> PathBuf {
    match std::env::var_os("LOCALAPPDATA") {
        Some(dir) => PathBuf::from(dir).join("bt-volume"),
        None => std::env::temp_dir().join("bt-volume"),
    }
}

pub fn log_path() -> PathBuf {
    data_dir().join("bt-volume.log")
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
