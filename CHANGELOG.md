## Changelog

Active since `0.1.0`.

### 0.1.11

- The list only redraws the rows on screen, so long histories no longer slow the display down.
- Moving up and down keeps the list still and moves only the cursor.
- Removed the green row flash on live updates.
- Latency keeps its real value instead of dropping to 0ms, and the header no longer shifts.
- History loads in the background, so switching tabs no longer shows Loading.
- Clicks on tabs, footer hints, password buttons and rows now match what is drawn, including after scrolling and in the split Search view.

### 0.1.10

- Trackpad and mouse wheel scrolling now scroll the details popup, and leave the list alone.
- The details popup stops scrolling at its last line and shows that line fully.

### 0.1.9

- Clicking a worker or job row opens its details popup, and the left and right arrows step to the previous or next item inside it.
- Removed wheel and trackpad scrolling.
- Added a CHECKED column showing when each worker last checked for updates.
- Columns now line up at every terminal width.
- Simplified the codebase by about a quarter and added rendering tests.

### 0.1.8

- Implemented real-time Server-Sent Events (SSE) stream (`/print/events`) eliminating periodic polling overhead.
- Instant 0ms event push on incoming print jobs, claims, and worker lifecycle updates.
- Added automatic stream reconnection with exponential backoff and seamless fallback synchronization.

### 0.1.7

- Implemented HTTP ETag conditional matching (`304 Not Modified`) with 0 payload bytes transferred on cached cycles.
- Added live active worker `CONNS` column and detail card metric.
- Added Tab password mask toggle (`show`/`hide`) with instant preview in authentication modal.
- Added Bubblezone-style interactive mouse hit-testing for tab switching, row selection, modal dismiss, action buttons, and wheel scrolling.
- Added full raw JSON inspector modal accessible via Enter, Space, or `v` with vertical scrolling.
- Added real-time ping / round-trip latency telemetry indicator.
- Added incoming print job pulse shimmer animation on new updates.
- Optimized intelligent polling to ping lightweight stats endpoints and fetch history only on job state mutations.
- Removed status dot indicators for clean typography.

### 0.1.6

- Simplified authentication modal to password-only input with blank username basic auth encoding.
- Enforced clean rustfmt formatting across codebase.

### 0.1.5

- Cleaned right header telemetry by removing redundant worker count display.

### 0.1.4

- Fixed credential persistence with reliable dual-storage across standard `~/.config/preprinttui/creds.json` and system keyring.
- Preserves credentials permanently across restarts without repeated prompts.

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
