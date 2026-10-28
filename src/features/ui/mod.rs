//! Optional egui window that lists every device remembered in the store.
//!
//! The window is created on demand: `app` parks in [`run_loop`] and only enters
//! eframe when the tray asks for a window.  This keeps the tray-only startup
//! path free of any GL context, and satisfies winit's requirement that the event
//! loop lives on the main thread.
//!
//! Everything the window shows comes from [`Shared`], which the audio engine
//! keeps up to date.  Its buttons execute commands from the shared registry.

mod fonts;
mod window;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use eframe::egui;

use crate::core::command::CommandRegistry;
use crate::core::state::Shared;
use crate::features::audio::AudioRequest;

enum Request {
    Show,
    Exit,
}

static SHARED: OnceLock<Arc<Shared>> = OnceLock::new();
static AUDIO: OnceLock<Sender<AudioRequest>> = OnceLock::new();
static COMMANDS: OnceLock<Arc<CommandRegistry>> = OnceLock::new();
static REQUESTS: OnceLock<Sender<Request>> = OnceLock::new();
static PENDING: Mutex<Option<Receiver<Request>>> = Mutex::new(None);
static CONTEXT: Mutex<Option<egui::Context>> = Mutex::new(None);

static OPEN: AtomicBool = AtomicBool::new(false);
static EXIT_REQUESTED: AtomicBool = AtomicBool::new(false);
static UNAVAILABLE: AtomicBool = AtomicBool::new(false);

/// Must be called once, before the tray starts.
pub fn init(shared: Arc<Shared>, audio: Sender<AudioRequest>, commands: Arc<CommandRegistry>) {
    let _ = SHARED.set(shared);
    let _ = AUDIO.set(audio);
    let _ = COMMANDS.set(commands);

    let (tx, rx) = mpsc::channel();
    let _ = REQUESTS.set(tx);
    if let Ok(mut slot) = PENDING.lock() {
        *slot = Some(rx);
    }
}

/// Shows the window, focusing it when it is already open.
pub fn request_show() {
    if OPEN.load(Ordering::SeqCst) {
        if let Some(ctx) = context() {
            ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
            ctx.request_repaint();
        }
        return;
    }

    if let Some(requests) = REQUESTS.get() {
        let _ = requests.send(Request::Show);
    }
}

/// Asks the process to end: closes the window, if any, and lets `run_loop` return.
pub fn request_exit() {
    EXIT_REQUESTED.store(true, Ordering::SeqCst);
    if let Some(ctx) = context() {
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        ctx.request_repaint();
    }
    if let Some(requests) = REQUESTS.get() {
        let _ = requests.send(Request::Exit);
    }
}

/// False when the window could not be created (for example, no GL context).
pub fn is_available() -> bool {
    !UNAVAILABLE.load(Ordering::SeqCst)
}

/// Runs on the main thread: waits for requests and opens the window when asked.
pub fn run_loop(show_initial: bool) {
    let Some(requests) = PENDING.lock().ok().and_then(|mut slot| slot.take()) else {
        return;
    };

    if show_initial {
        let _ = REQUESTS.get().map(|tx| tx.send(Request::Show));
    }

    loop {
        if EXIT_REQUESTED.load(Ordering::SeqCst) {
            break;
        }

        match requests.recv_timeout(Duration::from_millis(250)) {
            Ok(Request::Show) => {
                if EXIT_REQUESTED.load(Ordering::SeqCst) {
                    break;
                }
                window::run_window();
            }
            Ok(Request::Exit) | Err(RecvTimeoutError::Disconnected) => break,
            Err(RecvTimeoutError::Timeout) => {}
        }
    }
}

fn context() -> Option<egui::Context> {
    CONTEXT.lock().ok().and_then(|ctx| ctx.clone())
}
