# Contributing to preprinttui

Thank you for contributing to `preprinttui` and the PreConnect ecosystem.

## Getting Started

1. Clone the repository and install Rust (2024 edition or later).
2. Check your local build:
   ```bash
   cargo check
   cargo run --release
   ```

## Development & Verification

Before submitting a pull request, ensure all quality and lint checks pass:

```bash
cargo fmt --check
cargo clippy --release --all-targets -- -D warnings
cargo test --release
cargo build --release
```

## Pull Request Guidelines

- Keep pull requests focused on a single concern.
- Do not add comments or documentation strings to source code. Keep code clean and free of comments.
- Do not commit keys, passwords, credentials, or private student information.
- Follow snake_case naming for source files (maximum 2 words) and Title Case for workflow/documentation headings.
- Maintain strict clippy compliance (`unwrap_used = "deny"`, `unreachable = "deny"`, `unused_must_use = "deny"`).
