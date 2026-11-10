//! Tiny persistent volume store.
//!
//! Records are written as JSON so the file is easy to inspect and edit.  A
//! record is keyed by the audio endpoint id, which Windows keeps stable for the
//! lifetime of a paired Bluetooth device.
//!
//! Older releases used a tab separated file; [`Store::load`] still accepts that
//! format as a fallback so saved levels survive an upgrade.

mod entry;
mod units;

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

pub use entry::Entry;
pub use units::{percent, scalar_from_units, units_from_scalar, UNITS};

use entry::clean_name;

const STORE_VERSION: u32 = 1;
const DEFAULT_NAME: &str = "Bluetooth audio device";

#[derive(Serialize, Deserialize)]
struct StoreFile {
    #[serde(default)]
    version: u32,
    #[serde(default)]
    devices: Vec<Record>,
}

#[derive(Serialize, Deserialize)]
struct Record {
    id: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    level: u32,
}

pub struct Store {
    path: PathBuf,
    entries: BTreeMap<String, Entry>,
    dirty: bool,
}

impl Store {
    pub fn load(path: PathBuf) -> Self {
        let mut store = Self {
            path,
            entries: BTreeMap::new(),
            dirty: false,
        };

        let Ok(text) = fs::read_to_string(&store.path) else {
            return store;
        };

        // A corrupt or old file must not stop the program: fall back to the
        // legacy tab separated parser and keep whatever it can understand.
        let records = match serde_json::from_str::<StoreFile>(&text) {
            Ok(file) => file.devices,
            Err(_) => parse_legacy(&text),
        };

        for record in records {
            let id = record.id.trim();
            if id.is_empty() {
                continue;
            }
            let name = record.name.trim();
            store.entries.insert(
                id.to_string(),
                Entry {
                    name: if name.is_empty() {
                        DEFAULT_NAME.to_string()
                    } else {
                        name.to_string()
                    },
                    level: record.level.min(UNITS),
                },
            );
        }

        store
    }

    /// Looks up a saved level.  When the endpoint id is unknown (for example after
    /// re-pairing) a unique record with the same friendly name is used instead.
    /// The second tuple element is the old id, so the caller can migrate it.
    pub fn lookup(&self, id: &str, name: &str) -> Option<(u32, Option<String>)> {
        if let Some(entry) = self.entries.get(id) {
            return Some((entry.level, None));
        }

        let mut matches = self
            .entries
            .iter()
            .filter(|(_, entry)| entry.name.eq_ignore_ascii_case(name));
        let (old_id, entry) = matches.next()?;
        if matches.next().is_some() {
            return None; // ambiguous
        }
        Some((entry.level, Some(old_id.clone())))
    }

    /// Stores a level and returns `true` if the record actually changed.
    pub fn set(&mut self, id: &str, name: &str, level: u32) -> bool {
        let level = level.min(UNITS);
        let entry = Entry {
            name: clean_name(name),
            level,
        };
        match self.entries.get(id) {
            Some(existing) if existing.level == level && existing.name == entry.name => false,
            _ => {
                self.entries.insert(id.to_string(), entry);
                self.dirty = true;
                true
            }
        }
    }

    /// Moves a record from an old endpoint id to its current id.
    pub fn adopt(&mut self, old_id: &str, new_id: &str, name: &str, level: u32) {
        self.entries.remove(old_id);
        self.set(new_id, name, level);
        self.dirty = true;
    }

    pub fn flush(&mut self) -> io::Result<()> {
        if !self.dirty {
            return Ok(());
        }

        let file = StoreFile {
            version: STORE_VERSION,
            devices: self
                .entries
                .iter()
                .map(|(id, entry)| Record {
                    id: id.clone(),
                    name: clean_name(&entry.name),
                    level: entry.level,
                })
                .collect(),
        };

        let mut text = serde_json::to_string_pretty(&file)
            .map_err(io::Error::other)?;
        text.push('\n');

        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }

        let tmp = self.path.with_extension("tmp");
        fs::write(&tmp, text.as_bytes())?;
        fs::rename(&tmp, &self.path)?;
        self.dirty = false;
        Ok(())
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Saved records in id order.
    pub fn list(&self) -> impl Iterator<Item = (&str, &Entry)> {
        self.entries.iter().map(|(id, entry)| (id.as_str(), entry))
    }
}

/// Parses the tab separated format used before the JSON migration.
fn parse_legacy(text: &str) -> Vec<Record> {
    let mut records = Vec::new();
    for line in text.lines() {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut parts = line.splitn(3, '\t');
        let (Some(id), Some(level), Some(name)) = (parts.next(), parts.next(), parts.next()) else {
            continue;
        };
        let Ok(level) = level.trim().parse::<u32>() else {
            continue;
        };
        records.push(Record {
            id: id.to_string(),
            name: name.to_string(),
            level,
        });
    }
    records
}

pub fn default_path(dir: &Path) -> PathBuf {
    dir.join("devices.json")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_store(tag: &str) -> (PathBuf, Store) {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "sound-memory-test-{}-{tag}-{nanos}",
            std::process::id()
        ));
        let path = dir.join("devices.json");
        std::fs::create_dir_all(&dir).unwrap();
        let store = Store::load(path.clone());
        (dir, store)
    }

    #[test]
    fn parses_well_formed_records() {
        let (dir, _) = temp_store("parse");
        let path = dir.join("devices.json");
        std::fs::write(
            &path,
            r#"{
                "version": 1,
                "devices": [
                    {
                        "id": "{0.0.0.00000000}.{id}",
                        "name": "Headset (联想)",
                        "level": 9600
                    }
                ]
            }"#,
        )
        .unwrap();

        let store = Store::load(path);
        assert_eq!(store.len(), 1);
        let (id, entry) = store.list().next().unwrap();
        assert_eq!(id, "{0.0.0.00000000}.{id}");
        assert_eq!(entry.level, 9600);
        assert_eq!(entry.name, "Headset (联想)");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn skips_records_without_an_id() {
        let (dir, _) = temp_store("malformed");
        let path = dir.join("devices.json");
        std::fs::write(
            &path,
            r#"{"version":1,"devices":[{"id":"","level":500},{"id":"id","name":"name","level":500}]}"#,
        )
        .unwrap();

        let store = Store::load(path);
        assert_eq!(store.len(), 1);
        assert_eq!(store.list().next().unwrap().1.level, 500);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn load_accepts_the_legacy_tsv_format() {
        let (dir, _) = temp_store("legacy");
        let path = dir.join("devices.json");
        std::fs::write(&path, "id\t9600\tHeadset\nother\tnot-a-number\tbad\n").unwrap();

        let store = Store::load(path);
        assert_eq!(store.len(), 1);
        assert_eq!(store.list().next().unwrap().1.name, "Headset");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn clamps_out_of_range_levels() {
        let (dir, _) = temp_store("clamp-load");
        let path = dir.join("devices.json");
        std::fs::write(
            &path,
            r#"{"version":1,"devices":[{"id":"id","name":"loud","level":99999}]}"#,
        )
        .unwrap();

        let store = Store::load(path);
        assert_eq!(store.list().next().unwrap().1.level, UNITS);

        let mut store = store;
        store.set("other", "other", 12_345);
        assert_eq!(store.entries["other"].level, UNITS);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn defaults_empty_names() {
        let (dir, _) = temp_store("names");
        let path = dir.join("devices.json");
        std::fs::write(
            &path,
            r#"{"version":1,"devices":[{"id":"id","level":100}]}"#,
        )
        .unwrap();

        let store = Store::load(path);
        assert_eq!(
            store.list().next().unwrap().1.name,
            "Bluetooth audio device"
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn set_reports_only_real_changes() {
        let (dir, mut store) = temp_store("set");
        assert!(store.set("id", "Headset", 4200));
        assert!(!store.set("id", "Headset", 4200));
        assert!(store.set("id", "Headset", 4201));
        assert!(store.set("id", "Renamed", 4201));
        assert!(!store.set("id", "Renamed", 4201));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn lookup_prefers_endpoint_id() {
        let (dir, mut store) = temp_store("lookup-id");
        store.set("id-a", "Headset", 1000);
        store.set("id-b", "Headset", 2000);

        assert_eq!(store.lookup("id-b", "Headset"), Some((2000, None)));
        // Same name twice: no name-based recovery.
        assert_eq!(store.lookup("id-c", "Headset"), None);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn lookup_recovers_by_unique_name() {
        let (dir, mut store) = temp_store("lookup-name");
        store.set("old-id", "Fone de ouvido (联想)", 9600);

        assert_eq!(
            store.lookup("new-id", "Fone de ouvido (联想)"),
            Some((9600, Some("old-id".to_string())))
        );
        // Friendly names are compared case-insensitively.
        assert_eq!(
            store.lookup("new-id", "fone DE ouvido (联想)"),
            Some((9600, Some("old-id".to_string())))
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn lookup_ignores_unknown_names() {
        let (dir, mut store) = temp_store("lookup-unknown");
        store.set("id", "Headset", 500);
        assert_eq!(store.lookup("other", "Speakers"), None);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn adopt_moves_the_record_to_the_new_id() {
        let (dir, mut store) = temp_store("adopt");
        store.set("old-id", "Headset", 3300);
        store.adopt("old-id", "new-id", "Headset", 3300);

        assert_eq!(store.entries["new-id"].level, 3300);
        assert!(store.entries.contains_key("new-id"));
        assert!(!store.entries.contains_key("old-id"));
        assert_eq!(store.len(), 1);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn flush_round_trips_utf8_and_leaves_no_temp_file() {
        let (dir, mut store) = temp_store("flush");
        let path = dir.join("devices.json");
        store.set("id", "Headset (联想)", 9600);
        store.flush().unwrap();

        assert!(path.exists());
        assert!(!path.with_extension("tmp").exists());

        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.starts_with('{'));
        assert!(text.contains("\"version\": 1"));
        assert!(text.contains("\"name\": \"Headset (联想)\""));

        let reloaded = Store::load(path.clone());
        assert_eq!(reloaded.len(), 1);
        assert_eq!(reloaded.list().next().unwrap().1.level, 9600);
        assert_eq!(reloaded.list().next().unwrap().1.name, "Headset (联想)");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn flush_without_changes_writes_nothing() {
        let (dir, mut store) = temp_store("flush-clean");
        store.flush().unwrap();
        assert!(!dir.join("devices.json").exists());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn tabs_and_newlines_in_names_are_neutralised() {
        let (dir, mut store) = temp_store("sanitize");
        store.set("id", "Bad\tname\r\nhere", 100);
        store.flush().unwrap();

        let reloaded = Store::load(dir.join("devices.json"));
        let (_, entry) = reloaded.list().next().unwrap();
        assert_eq!(entry.name, "Bad name  here");
        std::fs::remove_dir_all(dir).unwrap();
    }
}
