# preprinttui Agent Guide

## Product

`preprinttui` is a standalone, lightweight Terminal User Interface (TUI) monitor for the PreConnect academic companion printer subsystem. It connects to the PreConnect printer endpoints (`/print/stats`, `/print/active`, `/print/history`) to inspect worker relays, monitor queue states, and search print job history in real time.

## Requirements

- Rust 2024 edition or newer
- Cargo
- `clippy` and `rustfmt`

## Setup & Run

```bash
cargo check
cargo run --release
```

## Build

```bash
cargo build --release
```

## Verification

Run after every code change:

```bash
cargo fmt --check
cargo clippy --release --all-targets -- -D warnings
cargo test --release
cargo build --release
```

## Repository Rules

- Preserve existing user changes and inspect the worktree before editing.
- Keep changes focused and avoid unrelated refactors.
- Do not add comments or documentation strings (such as JSDoc, inline explanations, or inline comments) to code. Keep code completely clean and free of comments.
- Do not store credentials in `.env` or disk files; all authentication is entered in-memory and held in RAM only.
- Use meaningful snake_case filenames with no more than two words (`types.rs`, `consts.rs`, `crypto.rs`, `app.rs`, `ui.rs`, `main.rs`).
- Use Title Case for document headings, workflow display names, issue-form names, and short labels. Use sentence case for descriptions, messages, release notes, and full questions.
- Maintain strict clippy compliance (`unwrap_used = "deny"`, `unreachable = "deny"`, `unused_must_use = "deny"`).
- Preserve GPL-3.0 notices and licensing.
