//! One record in the persistent store.

#[derive(Clone, Debug)]
pub struct Entry {
    pub name: String,
    pub level: u32,
}

/// Device names come from Windows and may contain separators used by the file
/// format, so tabs and line breaks are neutralised before writing.
pub(super) fn clean_name(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|ch| {
            if ch == '\t' || ch == '\r' || ch == '\n' {
                ' '
            } else {
                ch
            }
        })
        .collect();
    let cleaned = cleaned.trim();
    if cleaned.is_empty() {
        "Bluetooth audio device".to_string()
    } else {
        cleaned.to_string()
    }
}
