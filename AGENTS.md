# Agent Guidelines (`AGENTS.md`)

Welcome, coding agent! This document outlines the standards, architecture principles, and constraints you must follow when working in the `sound-memory` codebase.

## Core Architecture

- **Command Pattern**: Every user-visible feature must be implemented as a `Command` conforming to `core::command::Command`. Commands are stateless structs registered in `commands::standard()`.
- **Modular Features**: Code is organized into dedicated folders under `src/features/` (audio, tray, ui, store, autostart) and `src/core/` (command, state, util).
- **Thread Separation**: 
  - **Main Thread**: Owns the egui event loop (`ui::run_loop`) which runs on demand.
  - **Tray Thread**: Manages the hidden Win32 window, notification icon, context menu, and balloon notifications.
  - **Audio Engine Thread**: Handles all COM calls and Core Audio endpoint monitoring on a Multi-Threaded Apartment (MTA).

## Testing Rules (Mandatory)

1. **Every New Feature Must Include Tests**:
   - Any new command, policy rule, store capability, or utility function must be accompanied by comprehensive unit tests.
   - PRs or code changes that introduce logic without unit tests will not be accepted.
2. **Test Environment**:
   - Unit tests must be pure and runnable without requiring a physical Bluetooth audio device, an active Win32 desktop window, or interactive user intervention (`cargo test` must pass cleanly in headless/CI environments).
3. **Coverage & Validation**:
   - Place unit tests inside a `mod tests` block within the file where the code resides (following the existing pattern in `store`, `policy`, `state`, `util`, and `ui`).

## Coding Guidelines

- **Language**: All code comments, logs, and documentation must be in **English**.
- **Error Handling**: Use idiomatic Rust (`Result`, `Option`). Avoid unwrap/expect in production code paths unless startup guarantees it (e.g., single-instance or data directory setup).
- **Style**: Keep changes consistent with the surrounding code structure, naming conventions, and 2021 edition idioms.
