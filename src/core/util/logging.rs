//! Append-only diagnostic log with size-based rotation.

use std::io::Write;
use std::path::Path;
use std::sync::{Mutex, OnceLock};

use windows::Win32::System::SystemInformation::GetLocalTime;

use super::paths::ensure_parent;

static LOG: OnceLock<Mutex<std::fs::File>> = OnceLock::new();
const MAX_LOG_BYTES: u64 = 1_048_576;

pub fn init_logging(path: &Path) {
    if let Ok(meta) = std::fs::metadata(path) {
        if meta.len() > MAX_LOG_BYTES {
            let old = path.with_extension("log.1");
            let _ = std::fs::remove_file(&old);
            let _ = std::fs::rename(path, &old);
        }
    }

    ensure_parent(path);

    match std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    {
        Ok(file) => {
            let _ = LOG.set(Mutex::new(file));
        }
        Err(err) => eprintln!("sound-memory: cannot open log file {}: {err}", path.display()),
    }
}

pub fn log(message: &str) {
    let line = format!("{} {}\n", timestamp(), message);
    #[cfg(debug_assertions)]
    print!("{line}");

    if let Some(file) = LOG.get() {
        if let Ok(mut file) = file.lock() {
            let _ = file.write_all(line.as_bytes());
            let _ = file.flush();
        }
    }
}

fn timestamp() -> String {
    let st = unsafe { GetLocalTime() };
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02}.{:03}",
        st.wYear, st.wMonth, st.wDay, st.wHour, st.wMinute, st.wSecond, st.wMilliseconds
    )
}
