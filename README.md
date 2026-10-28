# sound-memory

A small, native Windows tray utility that remembers the volume level of every
Bluetooth audio device you use and restores it automatically when that device
connects again.

Written in Rust with the official `windows` bindings for the Win32 side and
[egui] for the optional devices window. No runtime, no installer, no background
service, no telemetry. The release executable is around 4.7 MB.

[egui]: https://github.com/emilk/egui


## What it does

- Watches Core Audio endpoint notifications for Bluetooth render devices.
- The first time a device is seen, it stores the current volume.
- Every time that device connects later, it restores the stored level.
- Keeps saving the level whenever you change it while the device is connected.
- Applies the stored level a few times right after a connection, because many
  headsets report their own AVRCP/absolute volume a moment after connecting and
  would otherwise overwrite the restored value.
- Shows every remembered device, its saved level and the current level in a
  small window built with egui.
- Lives in the notification area:
  - left click: open the devices window,
  - right click: show status, open the devices window, restore now, open the
    data folder or log, toggle **Start with Windows**, or exit.

## Build

Requirements:

- Windows 10 or 11 (x64)
- Rust stable with the MSVC toolchain

```powershell
cargo build --release
cargo test
```

The executable is `target\release\sound-memory.exe`.

The program has no administrator requirement. To use it from anywhere, copy the
executable to a folder of your choice and run it once.

## Usage

1. Start `sound-memory.exe`. An icon appears in the notification area; no window is
   created at startup.
2. Connect your Bluetooth headphones. The first connection stores the current
   level and shows a notification.
3. Set the volume you like. It is saved as you change it.
4. Disconnect and reconnect. The saved level is applied again.
5. Left click the tray icon (or use **Devices window…** in the menu) to see
   every remembered device with its saved and current level.

Start with `sound-memory.exe --show` to open the devices window immediately.

Use **Start with Windows** in the tray menu so the utility is running before
your headset connects after a reboot.

Only one instance runs at a time.

## Devices window

The window is built with egui and only exists while it is open: it is created on
demand on the main thread, and closing it returns to a tray-only process. It
lists every endpoint in `devices.tsv` with:

| Column | Meaning |
| --- | --- |
| Device | friendly name; hover to see the full name and endpoint id |
| Saved | level that will be restored on the next connection |
| Now | current level, when the device is connected |
| State | `connected` or `saved` |

Buttons: **Restore saved volumes**, **Open data folder**, **Open log**, and
**Hide** (closes the window without stopping the program).

## How it works

- `IMMDeviceEnumerator` enumerates active render endpoints. Endpoints whose
  device enumerator starts with `BTH` (Bluetooth A2DP, hands-free, LE Audio)
  are tracked.
- `IMMNotificationClient` and `IAudioEndpointVolumeCallback` provide
  connection and volume-change events without polling the Bluetooth stack.
- `IAudioEndpointVolume::SetMasterVolumeLevelScalar` applies the stored level.
- The store is keyed by the audio endpoint id, which Windows keeps stable for a
  paired device. If the id changes after re-pairing, a unique friendly-name
  match is migrated to the new id.
- Volume changes caused by the restore itself carry a private event context and
  are not stored again.
- Changes within three seconds of connecting are ignored for saving, since they
  are usually the headset's own volume sync rather than user intent.
- Three threads: the audio engine (COM MTA), the tray icon, and the main thread,
  which parks until the devices window is requested and then runs the egui event
  loop. The window is destroyed when closed, so a tray-only run never creates a
  GL context.

### Tests

`cargo test` runs the unit suite (34 tests, no Windows desktop required):

| Module | Covered behaviour |
| --- | --- |
| `features::store` | TSV parsing, malformed lines, clamping, name defaults/sanitising, change detection, id and name lookup, ambiguity, id migration, atomic flush round-trip, unit conversions |
| `features::audio::policy` | Bluetooth endpoint detection (including the empty instance id case), retry schedule, grace window decisions |
| `commands` | every feature is registered, command ids are unique |
| `core::command` | registry lookup and execution by id |
| `core::state` | saved-device snapshots, bounded notification queue |
| `core::util` | wide-string handling, surrogate pairs, truncation by characters |
| `features::ui` | saved/current level pairing for the device table |

### Files

| Path | Purpose |
| --- | --- |
| `%LOCALAPPDATA%\sound-memory\devices.tsv` | remembered levels, plain tab-separated text |
| `%LOCALAPPDATA%\sound-memory\sound-memory.log` | diagnostic log, rotated at 1 MB |
| `HKCU\Software\Microsoft\Windows\CurrentVersion\Run\BtVolume` | optional autostart entry |

The store looks like this (level is `0..10000`, i.e. hundredths of a percent):

```text
# sound-memory store v1 (level: 0..10000)
{0.0.0.00000000}.{...}	4200	Headphones (WH-1000XM4)
```

### Limitations and fine print

- The utility must be running to restore a level. Use autostart for a permanent
  setup.
- Each Windows audio endpoint has its own level. A headset that exposes both an
  A2DP and a hands-free endpoint is therefore remembered for each endpoint.
- The post-connect re-apply happens at 0, 0.6, 2.0 and 4.8 seconds (each delay is
  added to the previous attempt). If you change the volume during that window,
  the utility may overwrite it; changes made later are always recorded.
- Bluetooth speakers are handled the same way as headphones. Wired devices are
  ignored.
- If a headset is re-paired and the endpoint id changes, the level is recovered
  by friendly name only when that name is unique in the store.
- The store and log are plain files in the current user's profile; they contain
  device names and volume levels only.
- The devices window needs a working OpenGL context. If it cannot be created,
  the tray keeps working and the menu reports that the window is unavailable.

## Project layout

The application is split into infrastructure (`core`), user-visible features
(`commands`) and implementation modules (`features`). Every feature is an
executable command registered in `commands::standard()`:

```text
src/
  main.rs             thin entry point
  app.rs              composition root: store, registry, threads, shutdown
  core/
    command.rs        Command trait, CommandContext, CommandRegistry
    state.rs          Shared state between the three threads
    util/             paths, logging, wide strings, dialogs, single instance
  commands/
    mod.rs            registry with every feature command
    ids.rs            stable command identifiers
    show_status.rs    one file per feature command
    show_window.rs
    restore_all.rs
    open_data_folder.rs
    open_log_file.rs
    toggle_autostart.rs
    exit.rs
  features/
    audio/
      engine.rs       worker loop: track, save and restore levels
      endpoint.rs     Core Audio enumeration and device properties
      notifier.rs     COM callbacks that post channel events
      policy.rs       pure timing/Bluetooth policy (unit tested)
      request.rs      AudioRequest sent from commands to the engine
      com.rs          COM apartment guard
    tray/
      window.rs       hidden Win32 window and message loop
      icon.rs         Shell_NotifyIconW, tooltip, balloons, Notifier adapter
      menu.rs         context menu built from the command registry
    ui/
      mod.rs          on-demand egui loop and request channel
      window.rs       devices table and command buttons
      fonts.rs        CJK fallback fonts
    store/
      mod.rs          persistent TSV store
      entry.rs        one stored record
      units.rs        scalar <-> hundredths-of-a-percent conversions
    autostart/
      mod.rs          public API
      registry.rs     HKCU Run key access
```

### Command pattern

Each feature implements `core::command::Command`: a stable id, a menu label,
optional checked state and a `run(&CommandContext)` method. `CommandContext`
carries the shared state, the audio-engine sender and an optional tray notifier,
so the same command works from the tray menu and from the devices window. The
tray builds its menu from command ids, and the UI buttons execute those same
commands; adding a feature means adding one file under `src/commands` and
registering it.

## Uninstall

Exit from the tray menu, remove the executable, optionally delete
`%LOCALAPPDATA%\sound-memory`, and disable **Start with Windows** first if you
enabled it.
