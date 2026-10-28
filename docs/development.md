# Development Guide

This guide explains how to work locally on the `bt-volume` codebase, from building the project to running tests and making feature additions.

## Prerequisites

- Windows 10 or later (x64).
- The MSVC toolchain installed and `rustup` configured (`corepack enable` is not required for this project).
- For debugging: a physical display session or remote desktop (egui requires a GPU context on the main thread when the window runs).

## Build & Run

From the repository root:

```powershell
cargo build            # Debug executable under target/debug/bt-volume.exe
cargo build --release  # Optimized ~4.7 MB release binary; strip is enabled in [profile.release]
```

Run the release binary directly:

```powershell
.\target\release\bt-volume.exe
```

Optional startup flag `--show` forces the devices window to open immediately.

Run the debug binary to see logs in the console:

```powershell
.\target\debug\bt-volume.exe --show
```

## Tests

All logic is tested as pure unit tests that do not require hardware or an active UI session.

```powershell
cargo test           # Runs all unit tests (34 tests, zero Windows desktop required)
cargo test --no-run  # Compiles test binaries without running them
```

### Test Rules (Mandatory)

1. **Every new feature must include tests**:
   - New commands, policy rules, store capabilities, or utility functions require comprehensive unit tests.
   - PRs introducing new logic without corresponding unit tests will be rejected.
2. **Pure test constraints**:
   - Unit tests must not depend on Bluetooth endpoints, Win32 dialogs, or interactive user action.
   - Place each `mod tests { ... }` block alongside the implementation it validates, matching the existing pattern.

## Daily Workflow

| Step | Command / Action |
| --- | --- |
| Rebuild after changes | `cargo build` then `cargo test` |
| See only test results | `cargo test` |
| Fix a compiler error | Read error message, apply small edits, rebuild |
| Iterate quickly | Use `--release` build for timing changes only |

## Editor Tips

- Align tabs: follow Rust convention — no magic trailing whitespace.
- Add module documentation for every new file at its first line.
- If a folder is introduced, update both `src/features/mod.rs` (or equivalent per-module folder map) and `docs/architecture.md`.
