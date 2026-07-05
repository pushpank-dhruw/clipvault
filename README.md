# ClipVault

Lightweight clipboard history manager for Wayland with GPU-accelerated floating overlay.

## Features

- **Wayland-native monitoring** — polls clipboard via `wl-paste`, SHA-256 dedup
- **Floating overlay** — egui GPU-rendered window, Tokyo Night dark theme
- **Fuzzy search** — skim-based matching across history
- **History persistence** — SQLite with 500-entry auto-eviction
- **Hyprland native** — designed for Hyprland, frameless rounded overlay
- **IPC toggle** — Unix socket signal to show/hide the overlay
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
| `clipvault toggle` | Show/hide the overlay window |
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
poll_interval_ms = 500
theme = "tokyo-night"
overlay_width = 600
overlay_height = 450
```

## Tech Stack

**Rust** (edition 2024), **egui/eframe** (wgpu), **rusqlite** (SQLite), **wl-clipboard-rs**, **tokio**, **interprocess** (Unix sockets), **clap**, **tracing**.

## Status

Phase 1 (core) complete. Phase 2 (polish) in progress. Contributions welcome.
