# Architecture Overview

`sound-memory` is a native Windows system tray utility built in Rust. It remembers the volume level of every Bluetooth audio endpoint and restores it automatically upon reconnection.

## System Topology & Threads

The application runs three primary concurrent threads:

1. **Main Thread**:
   - Parks inside `ui::run_loop()`.
   - Opens and runs the `eframe`/`egui` devices window strictly on demand when requested via the tray icon or command line (`--show`).
   - When closed, the window is destroyed, returning the process to a lightweight tray-only state without an OpenGL context.

2. **Tray Thread**:
   - Creates a hidden Win32 message-only window (`BtVolumeTrayWindow`).
   - Registers the notification icon via `Shell_NotifyIconW`.
   - Dispatches context menu interactions and balloon notifications.

3. **Audio Engine Thread**:
   - Initializes a COM MTA apartment.
   - Uses `IMMDeviceEnumerator` and COM notification callbacks (`IMMNotificationClient`, `IAudioEndpointVolumeCallback`) to watch active render endpoints.
   - Communicates changes asynchronously via `mpsc` channels.

## The Command Pattern

To keep features decoupled and testable, every user-facing action is implemented as a command:

- Defined in `core::command::Command`:
  ```rust
  pub trait Command: Send + Sync {
      fn id(&self) -> CommandId;
      fn label(&self) -> &'static str;
      fn checked(&self) -> bool { false }
      fn run(&self, ctx: &CommandContext) -> CommandOutcome;
  }
  ```
- **Registry**: `commands::standard()` builds a centralized `CommandRegistry` containing all features (`ShowStatus`, `ShowWindow`, `RestoreAll`, `OpenDataFolder`, `OpenLogFile`, `ToggleAutostart`, `Exit`).
- **Context**: `CommandContext` exposes shared state (`Shared`), audio engine requests (`Sender<AudioRequest>`), and tray notification capabilities (`Notifier`).

## Directory Structure

```text
src/
  main.rs             Thin entry point
  app.rs              Composition root (threads, registry, startup/shutdown)
  core/
    command.rs        Command trait, context, and registry
    state.rs          Shared state across threads
    util/             Logging, paths, wide strings, dialogs, single-instance
  commands/           One file per feature command
  features/
    audio/            Core Audio engine, endpoints, callbacks, policies
    tray/             Notification icon, hidden window, context menu
    ui/               egui devices window and fallback CJK fonts
    store/            Persistent TSV volume store and scale conversions
    autostart/        Registry-based HKCU Run key integration
```
