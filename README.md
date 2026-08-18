<div align="center">

# `preprinttui`

Fast, interactive Terminal User Interface (TUI) for the [PreConnect](https://github.com/sabbirba/preconnect) printer subsystem.

[![GitHub Release](https://img.shields.io/github/v/release/sabbirba/preprinttui?label=latest%20version&color=dark-green&style=flat-square&logo=github)](https://github.com/sabbirba/preprinttui/releases/latest)
[![Rust](https://img.shields.io/badge/Rust-2024%20Edition-DEA584?style=flat-square&logo=rust)](https://www.rust-lang.org)
[![License](https://img.shields.io/badge/license-GPL3.0-blue?style=flat-square)](LICENSE)
[![Stars](https://img.shields.io/github/stars/sabbirba/preprinttui?style=flat-square&logo=github)](https://github.com/sabbirba/preprinttui/stargazers)
[![PreConnect](https://img.shields.io/badge/PreConnect-Ecosystem-02569B?style=flat-square)](https://github.com/sabbirba/preconnect)

</div>

---

### Overview

`preprinttui` is a standalone, lightweight terminal dashboard built with [Ratatui](https://github.com/ratatui/ratatui) to monitor and inspect the PreConnect printer swarm in real time. It communicates with the PreConnect printer endpoints (`/print/stats`, `/print/active`, `/print/history`) to deliver live telemetry, active worker relay status, and searchable print history.

### Features

- **Inline Real-Time Header Metrics**: Persistent live cluster telemetry inline in the top header (`ONLINE • Workers • Uptime • Queue • History`).
- **Full-Height Active Workers & History**: Viewport dedicated to active worker relays and searchable print job history with zero clipping across all screen sizes.
- **Cross-Platform Universal Secure Storage**: Credentials saved in OS keychain (macOS Keychain, Linux Secret Service, Windows Credential Manager) with fallback to restricted user-only storage (`0600`) on Android Termux and headless servers.
- **Adaptive Single-Line Details**: Inspector rows dynamically expand on a single non-wrapping row when horizontal space is available.
- **High-Precision Monotonic Timer**: 20 FPS jitter-free clock ticking smoothly with sub-second accuracy.
- **Zero Overhead**: Fully optimized release binary compiled with LTO, abort panic, and stripped symbols.

---

### Installation & Compiling

#### From Source

Requires [Rust](https://rust-lang.org/) (2024 edition or later):

```bash
git clone https://github.com/sabbirba/preprinttui.git
cd preprinttui
cargo build --release
```

The optimized binary will be available at `target/release/preprinttui`.

#### Global Installation via Cargo

```bash
cargo install --path .
```

#### Prebuilt Binaries

Precompiled standalone binaries for Windows (`x86_64`), Linux (`x86_64-unknown-linux-musl`), Android (Termux), and macOS (`arm64`) are available on the [GitHub Releases](https://github.com/sabbirba/preprinttui/releases) page.

---

### Usage

```bash
./preprinttui
```

---

### Authentication Workflow

1. Start `preprinttui`:
   ```bash
   ./preprinttui
   ```
2. Public statistics load automatically in the inline header.
3. If credentials were previously saved, `preprinttui` loads them securely from the system keychain / config store and authenticates immediately.
4. To view or change protected active workers and job history credentials, press **`e`** (or **`c`**) to open the Credential Modal.
5. Type your `WORKER_KEY` or `PASSWORD` (inputs are securely masked).
6. Press **`Enter`** to save credentials securely and authenticate immediately.
7. Press **`x`** at any time to purge credentials from secure storage.

---

### Keyboard Controls

| Key | Action |
|---|---|
| `1` | Switch to **Active Workers** tab |
| `2` | Switch to **Job History** tab |
| `Tab` / `Left` / `Right` | Cycle between tabs |
| `e` / `c` | Open **Credential Modal** |
| `x` | **Clear Credentials** from secure storage |
| `Down` / `j` | Move selection down in active table |
| `Up` / `k` | Move selection up in active table |
| `/` | Enter search / filter mode in history |
| `Esc` / `Enter` | Exit search or cancel modal |
| `r` | Trigger manual refresh |
| `a` | Toggle auto-refresh on/off |
| `q` / `Ctrl+C` | Quit application |

---

### Credits & Acknowledgments

- **Author**: [Sabbir Bin Abbas](https://github.com/sabbirba)
- **PreConnect Ecosystem**: Developed as part of the [PreConnect](https://github.com/sabbirba/preconnect) academic companion platform for BRAC University students.
- **Daemon & Swarm Protocol**: Inspired by and compatible with [`hitblast/preprintd`](https://github.com/hitblast/preprintd) by [Anindya Shiddhartha](https://github.com/hitblast).
- **TUI Framework**: Powered by [Ratatui](https://github.com/ratatui/ratatui) and [Crossterm](https://github.com/crossterm-rs/crossterm).

---

### License

This project is licensed under the [GNU General Public License v3.0](LICENSE).
