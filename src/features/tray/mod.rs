//! Notification-area feature: hidden window, icon, menu and balloons.

mod icon;
mod menu;
mod window;

pub use window::{request_exit, run};
