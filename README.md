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

### Arch Linux / Omarchy (AUR)

```bash
yay -S clipvault

# autostart the daemon with your session
systemctl --user enable --now clipvault

# Hyprland keybind + window rules (add to ~/.config/hypr/hyprland.conf)
echo 'source = /usr/share/clipvault/hyprland-clipvault.conf' >> ~/.config/hypr/hyprland.conf
hyprctl reload
```

On Omarchy you can put the SUPER+SHIFT+V bind in `~/.config/hypr/bindings.conf`
instead; the packaged rules file only carries window rules if you prefer that split.

### From source

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

The AUR package ships a ready-made config at
`/usr/share/clipvault/hyprland-clipvault.conf` (keybind + window rules for the
`clipvault` host and `clipvault-shelf` windows). Source it as shown above, or
copy what you need:

```ini
# keybind
bind = SUPER SHIFT, V, exec, clipvault toggle

# notch shelf: floating top-center bar
windowrule = float on, match:class ^(clipvault-shelf)$
windowrule = size 800 140, match:class ^(clipvault-shelf)$
windowrule = move 50%-400 8, match:class ^(clipvault-shelf)$
windowrule = stay_focused on, match:class ^(clipvault-shelf)$
windowrule = rounding 12, match:class ^(clipvault-shelf)$

# invisible 2x2 host window (required by the Wayland windowing model)
windowrule = float on, match:class ^(clipvault)$
windowrule = size 2 2, match:class ^(clipvault)$
windowrule = opacity 0, match:class ^(clipvault)$
windowrule = no_focus on, match:class ^(clipvault)$
```

Prefer the systemd user service over `exec-once` for autostart:
`systemctl --user enable --now clipvault`.

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
