<div align="center">

# `preprinttui`

Fast, interactive Terminal User Interface (TUI) for the [PreConnect](https://github.com/sabbirba/preconnect) printer subsystem, crafted with Charm & Bubble Tea design principles.

[![GitHub Release](https://img.shields.io/github/v/release/sabbirba/preprinttui?label=latest%20version&color=dark-green&style=flat-square&logo=github)](https://github.com/sabbirba/preprinttui/releases/latest)
[![Rust](https://img.shields.io/badge/Rust-2024%20Edition-DEA584?style=flat-square&logo=rust)](https://www.rust-lang.org)
[![License](https://img.shields.io/badge/license-GPL3.0-blue?style=flat-square)](LICENSE)
[![Stars](https://img.shields.io/github/stars/sabbirba/preprinttui?style=flat-square&logo=github)](https://github.com/sabbirba/preprinttui/stargazers)
[![PreConnect](https://img.shields.io/badge/PreConnect-Ecosystem-02569B?style=flat-square)](https://github.com/sabbirba/preconnect)

</div>

---

### Overview

`preprinttui` is a standalone, ultra-fast terminal dashboard inspired by [Charm](https://charm.land/) and [Bubble Tea](https://github.com/charmbracelet/bubbletea) to monitor and inspect the PreConnect printer swarm in real time. It connects directly with the PreConnect printer endpoints (`/print/stats`, `/print/active`, `/print/history`) to deliver live telemetry, active worker relay status, and instant searchable print records.

### Features

- **Charm & Bubble Tea Design**: High-contrast pill tabs, Bubbles textinput prompt, Huh modal dialogs, and minimalist help keymaps.
- **Micro-Animations & Real-Time Engine**: 20 FPS live Braille spinner (`⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏`), blinking cursor, dynamic status badge toasts (`✓ Saved`, `✓ Refreshed`), and sub-second uptime ticking.
- **Parallel Async Concurrency**: Concurrent multi-endpoint fetching via `tokio::join!` cutting round-trip latency in half.
- **Dedicated Search Tab & Live Search**: Search across active cluster workers and job history with real-time query matching directly in the tab row.
- **Adaptive Inspector Card**: Dynamically responsive bottom inspector card preventing any row overflow or clipping across all terminal widths.
- **Cross-Platform Secure Storage**: Zero-plaintext secret management via OS keychain (macOS Keychain, Linux Secret Service, Windows Credential Manager) with fallback to restricted user-only files (`0600`).
- **Single-Word Interface**: Clean, clutter-free minimalist typography.

---

### Installation

#### Homebrew (macOS & Linux)

```bash
brew install sabbirba/tap/preprinttui
```

To update anytime:
```bash
brew upgrade preprinttui
```

#### One-Line Installer (macOS, Linux & Android Termux)

```bash
curl -fsSL https://raw.githubusercontent.com/sabbirba/preprinttui/main/install.sh | bash
```

#### Cargo

```bash
cargo install preprinttui
```

#### From Source

Requires [Rust](https://rust-lang.org/) (2024 edition or later):

```bash
git clone https://github.com/sabbirba/preprinttui.git
cd preprinttui
cargo build --release
```

The optimized binary will be available at `target/release/preprinttui`.

#### Prebuilt Binaries

Precompiled standalone binaries for Windows (`x86_64`), Linux (`x86_64-unknown-linux-musl`), Android (Termux), and macOS (`arm64`) are available on the [GitHub Releases](https://github.com/sabbirba/preprinttui/releases) page.

---

### Usage

```bash
preprinttui
```

---

### Authentication Workflow

1. Launch `preprinttui`:
   ```bash
   preprinttui
   ```
2. Public statistics load automatically.
3. If credentials were previously saved, `preprinttui` loads them securely and authenticates immediately.
4. To view or change protected active workers and history credentials, press **`e`** (or **`c`**) to open the Credentials modal.
5. Enter your `Key` or `Password` (inputs are securely masked).
6. Press **`Enter`** to save credentials securely and authenticate immediately.
7. Press **`x`** at any time to purge credentials from secure storage.

---

### Keyboard Controls

| Key | Action |
|---|---|
| `1` | Switch to **Workers** tab |
| `2` | Switch to **History** tab |
| `3` | Switch to **Search** tab |
| `Tab` / `l` / `Right` | Cycle forward through tabs |
| `BackTab` / `h` / `Left` | Cycle backward through tabs |
| `j` / `Down` | Move selection down |
| `k` / `Up` | Move selection up |
| `/` | Open live **Search** on active view |
| `e` / `c` | Open **Credentials** modal |
| `x` | **Clear** credentials from storage |
| `r` | Trigger **Refresh** |
| `a` | Toggle **Auto-refresh** |
| `q` / `Ctrl+C` | **Quit** |

---

### Credits & Acknowledgments

- **Author**: [Sabbir Bin Abbas](https://github.com/sabbirba)
- **PreConnect Ecosystem**: Developed as part of the [PreConnect](https://github.com/sabbirba/preconnect) academic companion platform for BRAC University students.
- **Daemon & Swarm Protocol**: Inspired by and compatible with [`hitblast/preprintd`](https://github.com/hitblast/preprintd) by [Anindya Shiddhartha](https://github.com/hitblast).
- **Design Inspiration**: [Charm](https://charm.land/) and [Bubble Tea](https://github.com/charmbracelet/bubbletea).

---

### License

This project is licensed under the [GNU General Public License v3.0](LICENSE).
