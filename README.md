# ClipVault

Lightweight clipboard history manager for Wayland with a GPU-accelerated notch shelf UI.

## Features

- **Wayland-native monitoring** — polls clipboard via `wl-paste`, SHA-256 dedup
- **Notch shelf** — egui GPU-rendered top-center bar (800x140), Tokyo Night dark theme
- **Fuzzy search** — skim-based matching across history
- **History persistence** — SQLite with 500-entry auto-eviction
- **Image clipboard support**: PNG capture, shelf thumbnails, files stored on disk
- **Hyprland native** — designed for Hyprland, frameless rounded shelf
- **IPC toggle** — Unix socket commands to show/hide the shelf, query status, quit
- **Clipboard source tracking** — captures which app copied the content

## Installation

```bash
cargo build --release
sudo cp target/release/clipvault /usr/local/bin/
```

## Usage

| Command | Description |
|---|---|
| `clipvault` | Start daemon (background monitoring + GUI) |
| `clipvault toggle` | Show/hide the clipboard shelf |
| `clipvault quit` | Quit the running daemon |
| `clipvault list` | Print history as JSON |
| `clipvault search <query>` | Search from terminal |
| `clipvault clear` | Clear all history |
| `clipvault status` | Daemon status + entry count |

## Hyprland Integration

```ini
# keybind
bind = SUPER SHIFT, V, exec, clipvault toggle

# autostart
exec-once = clipvault

# window rules
windowrule = opacity 0.95, match:class ^(clipvault)$
windowrule = no_blur on, match:class ^(clipvault)$
windowrule = rounding 12, match:class ^(clipvault)$
windowrule = no_shadow on, match:class ^(clipvault)$
windowrule = stay_focused on, match:class ^(clipvault)$
```

## Configuration

`~/.config/clipvault/config.toml`:

```toml
max_entries = 500
max_image_entries = 50
poll_interval_ms = 500
theme = "tokyo-night"
shelf_width = 800.0
shelf_height = 140.0
shelf_thumb_size = 56.0
shelf_max_entries = 50
ocr_enabled = false
hide_sensitive = false
image_store_dir = "images"
```

## Tech Stack

**Rust** (edition 2024), **egui/eframe** (wgpu), **rusqlite** (SQLite), **wl-clipboard-rs**, **tokio**, **interprocess** (Unix sockets), **clap**, **tracing**.

## Status

Phase 1 (core) complete. Phase 2 (polish) in progress. Contributions welcome.
