## Changelog

Active since `0.1.0`.

### 0.1.3

- Redesigned with Charm and Bubble Tea design system and Lip Gloss monochrome aesthetics.
- Added live 20 FPS animations including Braille spinner, blinking cursor, and status badge toasts.
- Introduced dedicated Search tab with automatic query focus and multi-source worker and history matching.
- Made inspector card responsive with zero overflow or text clipping across all viewport sizes.
- Fixed basic authentication header encoding for seamless authorization against PreConnect API.
- Replaced all multi-word titles and actions with strict single-word labels throughout.

### 0.1.2

- Removed document print names/filenames from history view and inspector for student privacy.
- Optimized column allocation across all terminal viewport breakpoints.

### 0.1.1

- Fully purged blue and yellow colors across the entire user interface.
- Adopted clean modern monochrome neutral palette with soft dark charcoal row selection.
- Refined table headers, uptime telemetry, worker inspector, and job history details.
- Clean release notes and commit tracking architecture.

### 0.1.0

- Initial release of `preprinttui` for the PreConnect printer subsystem.
- Real-time cluster status, uptime, worker count, queue metrics, and lifetime processed jobs overview.
- Active worker relay inspector with heartbeat, IP, user-agent, and completed jobs.
- Searchable and filterable print job history browser.
- Universal multi-tier secure storage (OS Keyring + chmod 0600 file fallback).
- Responsive non-clipping adaptive TUI with neutral color palette.
- Explicit HTTP 401 Unauthorized visual status handling.
