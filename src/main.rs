#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod commands;
mod core;
mod features;

use std::process::ExitCode;

fn main() -> ExitCode {
    app::run()
}
