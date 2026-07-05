# ClipVault — Omarchy Clipboard Manager

Lightweight, fast clipboard history with Wayland-native monitoring and egui floating overlay.
Designed for the Omarchy Functional aesthetic.

## Tech Stack

- **Language**: Rust (edition 2024)
- **GUI**: egui/eframe (GPU-accelerated, Wayland-native, floating overlay)
- **CLI**: clap (derive macros)
- **Clipboard**: wl-clipboard-rs (Wayland)
- **Storage**: rusqlite (SQLite, bundled)
- **IPC**: interprocess (Unix domain sockets for toggle signal)
- **Async**: tokio (full runtime)
- **Config**: toml + serde (XDG paths via directories crate)
- **Errors**: anyhow (binary) + thiserror (library)
- **Logging**: tracing + tracing-subscriber (env-filter)

## Commands

```bash
cargo build --release           # Release build (LTO, strip, 1 codegen unit)
cargo check                     # Fast check only (dev cycle)
cargo run                       # Run daemon (background + GUI on demand)
cargo run -- toggle             # Toggle GUI window (sends Unix socket signal)
cargo run -- list               # Print history as JSON
cargo run -- list --format json # Explicit JSON output
cargo run -- search <query>     # Search from terminal
cargo run -- clear              # Clear all history
cargo run -- status             # Daemon status + count
cargo test                      # All tests
cargo clippy --all-targets --all-features --locked -- -D warnings
cargo fmt                       # Format (must pass before commit)
```

## Code Style

- Follow `rustfmt` defaults
- `cargo clippy` must pass with `-D warnings` (deny mode)
- Never use `unwrap()`/`expect()` outside tests
- `anyhow::Result` for binary code, `thiserror` for reusable errors
- Structured as library crate + binary:
  - `src/lib.rs` — public API, re-exports modules
  - `src/main.rs` — CLI parsing + daemon entry, thin
- All public items documented with `///` doc comments
- Tests in `tests/` directory + doc tests for public API
- One assertion per test, descriptive names (`should_*`)

## Architecture

```
src/
├── main.rs     # CLI entry (clap) + daemon lifecycle
├── lib.rs      # Public API, module re-exports
├── monitor.rs  # Wayland clipboard monitoring (wl-clipboard-rs, 500ms poll)
├── store.rs    # SQLite CRUD (rusqlite, auto-eviction, search)
├── gui.rs      # egui floating overlay (frameless, dark theme, search)
├── config.rs   # ~/.config/clipvault/config.toml (serde)
└── ipc.rs      # Unix domain sockets (interprocess, toggle signal)
```

## Data Flow

```
Wayland clipboard changes
  → monitor.rs captures text + source + timestamp
  → store.rs inserts into SQLite (dedup by SHA-256 hash)
  → egui window polls DB and renders scrollable timeline
  → User clicks entry → copied back to clipboard
  → User presses Esc → window hides (daemon stays alive)
```

## Runtime Model

Single binary, two modes:
1. **Daemon** (default): background clipboard capture + Unix socket listener. egui window hidden until toggled.
2. **Toggle** (`clipvault toggle`): sends signal via Unix socket → daemon shows/hides egui overlay.

No separate server process. The daemon IS the GUI process, it just keeps the window hidden.

## Design (Omarchy Functional)

- **Dark bg**: `#1a1b26` (Tokyo Night background)
- **Accent**: `#7aa2f7` (Tokyo Night blue)
- **Text**: `#a9b1d6` (Tokyo Night foreground)
- **Selection**: `#c0caf5` on `#7aa2f7`
- **Frameless** window with rounded corners via Hyprland `windowrule`
- **Floating overlay** centered on active monitor (600×450, like rofi/walker)
- **Hyprland blur**: `windowrule = opacity 0.95, match:class ^(clipvault)$` + `windowrule = no_blur on, match:class ^(clipvault)$`
- **Terminal output**: tabular numbers, box-drawing borders (`┌┐│└┘`), no splash

## Hyprland Integration

```bash
# ~/.config/hypr/bindings.conf
bind = SUPER SHIFT, V, exec, clipvault toggle

# ~/.config/hypr/hyprland.conf (window rules — Hyprland 0.53+ syntax)
windowrule = opacity 0.95, match:class ^(clipvault)$
windowrule = no_blur on, match:class ^(clipvault)$
windowrule = rounding 12, match:class ^(clipvault)$
windowrule = no_shadow on, match:class ^(clipvault)$
windowrule = stay_focused on, match:class ^(clipvault)$

# ~/.config/hypr/autostart.conf
exec-once = clipvault
```

## Installation

```bash
cargo build --release
sudo cp target/release/clipvault /usr/local/bin/
```

## Data Locations

| Purpose | Path |
|---|---|
| Config | `~/.config/clipvault/config.toml` |
| Database | `~/.local/share/clipvault/clipvault.db` |
| IPC socket | `/run/user/$UID/clipvault.sock` |
| Log | stderr (journald/terminal) |

## Config (`~/.config/clipvault/config.toml`)

```toml
max_entries = 500
poll_interval_ms = 500
theme = "tokyo-night"
overlay_width = 600
overlay_height = 450
