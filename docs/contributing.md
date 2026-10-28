# Contributing to `sound-memory`

Thank you for your interest in improving this project! This document defines the process and standards for contributing code, documentation, and tests.

## Getting Started

1. **Fork the repository** and create a new branch from `main` for your feature or fix.
   ```powershell
   git checkout -b feature/your-feature-name
   ```
2. **Set up the development environment** (Windows + Rust toolchain). See `docs/development.md`.
3. **Run the existing test suite** to confirm a clean baseline:
   ```powershell
   cargo test
   ```
4. **Implement your change** following the project's architecture (Command Pattern) and code style.

## Development Rules

### 1. Language
- All code, comments, log messages, and documentation **must be written in English**.
- All new files and edits to existing files must use UTF-8 encoding.

### 2. Architecture Compliance
- Every new user-visible capability must be implemented as a new Command in `src/commands/`.
- Register every new Command in `commands::standard()` in `src/commands/mod.rs`.
- Do not bypass the `CommandContext` / `CommandRegistry` abstraction to access shared state directly from menus or UI.

### 3. Tests (Mandatory)
- **Every new feature must include tests**.
  - Any new command, policy rule, store capability, or utility function must have corresponding unit tests in a `mod tests` block.
  - PRs that add logic without tests will not be merged.
- Tests must be pure (runnable via `cargo test` without devices, UI, or interactive input).
- Follow the existing test patterns: use `mod tests`, assert pure functions, and avoid platform-dependent side effects.

### 4. Code Style
- Follow Rust 2021 edition idioms.
- Use spaces for indentation (2 spaces is conventional, but the existing code uses tabs in some places; keep existing style consistent within each file).
- Document public APIs with a brief comment at the file/module level.
- Avoid `unwrap()` or `expect()` in production logic paths unless a startup guarantee applies.

## Submission Flow

1. **Write your code** and add unit tests that cover the new functionality.
2. **Run the full test suite** to ensure nothing is broken:
   ```powershell
   cargo test
   ```
3. **Commit your changes** with a descriptive message following the existing convention (e.g., `feat: add command to toggle autostart`).
4. **Push your branch** and open a Pull Request (PR) against the `main` branch.
5. The PR will be reviewed. Reviewers may request changes or ask for additional tests. All requested changes must be addressed before merging.

## Directory Map Update

If you add a new module folder (e.g., a new feature under `src/features/`), you must update the module map accordingly (usually `src/features/mod.rs` or the parent folder's equivalent) to keep `cargo check` and builds happy.