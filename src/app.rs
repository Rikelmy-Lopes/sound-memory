//! Composition root: loads state, builds the command registry and starts the
//! three threads (main/UI, tray, audio engine).

use std::process::ExitCode;
use std::sync::{mpsc, Arc};

use crate::commands;
use crate::core::state::Shared;
use crate::core::util;
use crate::features::audio::{self, AudioRequest};
use crate::features::{store, tray, ui};

pub fn run() -> ExitCode {
    let data_dir = match util::ensure_data_dir() {
        Ok(dir) => dir,
        Err(err) => {
            util::show_message(
                &format!("sound-memory could not create its data folder:\n{err}"),
                "sound-memory",
                true,
            );
            return ExitCode::FAILURE;
        }
    };

    let log_file = util::log_path();
    util::init_logging(&log_file);
    util::log("sound-memory starting");

    let _instance = match util::acquire_single_instance() {
        Ok(Some(handle)) => Some(handle),
        Ok(None) => {
            util::show_message(
                "sound-memory is already running.\n\nLook for its icon in the notification area.",
                "sound-memory",
                false,
            );
            return ExitCode::SUCCESS;
        }
        Err(err) => {
            util::log(&format!("single-instance check failed: {err}"));
            None
        }
    };

    let store_path = store::default_path(&data_dir);
    let store = store::Store::load(store_path);
    util::log(&format!("{} saved device(s)", store.len()));
    let shared = Arc::new(Shared::new(store, data_dir, log_file));

    // Every user-visible feature is a command in this registry.
    let registry = Arc::new(commands::standard());
    util::log(&format!("{} command(s) registered", registry.len()));

    let (audio_tx, audio_rx) = mpsc::channel::<AudioRequest>();
    // The UI channel exists before the tray starts, so the first click is never lost.
    ui::init(
        Arc::clone(&shared),
        audio_tx.clone(),
        Arc::clone(&registry),
    );

    let engine_shared = Arc::clone(&shared);
    let engine = std::thread::Builder::new()
        .name("audio-engine".to_string())
        .spawn(move || audio::run(engine_shared, audio_rx));

    let engine = match engine {
        Ok(engine) => Some(engine),
        Err(err) => {
            util::log(&format!("could not start audio thread: {err}"));
            util::show_message(
                &format!("sound-memory could not start its audio engine:\n{err}"),
                "sound-memory",
                true,
            );
            None
        }
    };

    // The tray lives on its own thread because the egui event loop must own the
    // main thread.  The tray asks the UI thread to exit when the user picks Exit.
    let tray_shared = Arc::clone(&shared);
    let tray_audio = audio_tx.clone();
    let tray_commands = Arc::clone(&registry);
    let tray = std::thread::Builder::new()
        .name("tray".to_string())
        .spawn(move || {
            if let Err(err) = tray::run(tray_shared, tray_audio, tray_commands) {
                util::log(&format!("tray failed: {err}"));
                util::show_message(
                    &format!("sound-memory could not create its tray icon:\n{err}"),
                    "sound-memory",
                    true,
                );
                ui::request_exit();
            }
        })
        .map_err(|err| util::log(&format!("could not start tray thread: {err}")));

    // Main thread: the egui window is opened on demand, so startup stays
    // tray-only.  run_loop parks here until the tray asks to exit.
    let show_initial = std::env::args().any(|arg| arg == "--show");

    if tray.is_err() {
        util::log("no tray thread; exiting");
    } else {
        ui::run_loop(show_initial);
    }

    tray::request_exit();
    let _ = audio_tx.send(AudioRequest::Shutdown);

    if let Ok(tray) = tray {
        let _ = tray.join();
    }
    if let Some(engine) = engine {
        let _ = engine.join();
    }

    util::log("sound-memory stopped");
    ExitCode::SUCCESS
}
