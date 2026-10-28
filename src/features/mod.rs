//! Feature modules.
//!
//! Each folder owns one area of the application and exposes a small public API:
//!
//! - [`audio`] — Core Audio monitoring and volume restoration (worker thread).
//! - [`tray`] — notification-area icon, menu and balloons.
//! - [`ui`] — optional egui devices window (main thread).
//! - [`store`] — persistent per-device volume store.
//! - [`autostart`] — `HKCU\...\Run` integration.

pub mod audio;
pub mod autostart;
pub mod store;
pub mod tray;
pub mod ui;
