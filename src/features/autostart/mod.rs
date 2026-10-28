//! "Start with Windows" support via `HKCU\...\Run`.

mod registry;

pub use registry::{is_enabled, set_enabled};
