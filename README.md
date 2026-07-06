# ClipVault

Clipboard history for Hyprland. It lives at the top of the screen and drops
down when you reach for it, so you can grab something you copied earlier
without losing your place.

I built it for my own Omarchy machine, so it leans hard on Hyprland and
Wayland. That's where it works.

## What it does

Hover the top center of the screen (or press your keybind) and the shelf
slides down. It keeps everything you copy, text and images, and lays it out
as cards you scroll through. Click a card to copy it back.

A few things worth knowing:

- Cards are grouped by day, so Today and Yesterday stay separate.
- Hex colors like `#FF8F4A` show up as color swatches.
- The OCR button pulls text out of a screenshot, if you have tesseract.
- Each card shows the app you copied from, how long ago, and image sizes.
- Hover a card to favorite it, delete it, or open the image.
- Search when the list gets long.

## Install

### Arch / Omarchy

```bash
yay -S clipvault
systemctl --user enable --now clipvault
echo 'source = /usr/share/clipvault/hyprland-clipvault.conf' >> ~/.config/hypr/hyprland.conf
hyprctl reload
```

On Omarchy the `SUPER+SHIFT+V` bind fits in `~/.config/hypr/bindings.conf` if
you keep your keybinds together.

### From source

```bash
cargo build --release
sudo cp target/release/clipvault /usr/local/bin/
```

The OCR button needs tesseract:

```bash
sudo pacman -S tesseract tesseract-data-eng
```

## Commands

| Command | What it does |
|---|---|
| `clipvault` | Start the daemon (capture plus shelf) |
| `clipvault toggle` | Show or hide the shelf |
| `clipvault quit` | Stop the daemon |
| `clipvault list` | Print history as JSON |
| `clipvault search <query>` | Search from the terminal |
| `clipvault clear` | Wipe the history |
| `clipvault status` | Daemon status and entry count |

## Hyprland

The package ships a ready config at
`/usr/share/clipvault/hyprland-clipvault.conf` with the keybind and window
rules. Source it, or copy the parts you want:

```ini
bind = SUPER SHIFT, V, exec, clipvault toggle

# the shelf
windowrule = float on, match:class ^(clipvault-shelf)$
windowrule = size 820 220, match:class ^(clipvault-shelf)$
windowrule = move 50%-410 8, match:class ^(clipvault-shelf)$
windowrule = no_initial_focus on, match:class ^(clipvault-shelf)$
windowrule = rounding 14, match:class ^(clipvault-shelf)$
windowrule = animation slide top, match:class ^(clipvault-shelf)$
windowrule = opacity 1.0 override 1.0 override, match:class ^(clipvault-shelf)$

# hidden 2x2 host window the daemon needs
windowrule = float on, match:class ^(clipvault)$
windowrule = size 2 2, match:class ^(clipvault)$
windowrule = opacity 0, match:class ^(clipvault)$
windowrule = no_focus on, match:class ^(clipvault)$
```

`no_initial_focus` matters: it stops a hover-opened shelf from stealing
keyboard focus while you type. The daemon grabs focus itself when you open
the shelf with the keybind.

## Config

Lives at `~/.config/clipvault/config.toml`. The defaults are fine; change
them if you want:

```toml
max_entries = 500
max_image_entries = 50
poll_interval_ms = 500
theme = "tokyo-night"
shelf_width = 820.0
shelf_height = 220.0
shelf_thumb_size = 56.0
shelf_max_entries = 50
notch_hover = true                # drop the shelf when you hover the notch
notch_hover_width = 300.0         # hot zone width, centered at the top edge
notch_hover_height = 8.0          # minimum hot zone height (a top bar extends it)
notch_hover_dwell_ms = 120        # how long to dwell before it opens
notch_hover_close_delay_ms = 400  # grace period before a hover-opened shelf hides
notch_hover_poll_ms = 90          # cursor poll interval
ocr_enabled = false
hide_sensitive = false
image_store_dir = "images"
```

## Built with

Rust, egui/eframe on wgpu, rusqlite, wl-clipboard-rs, tokio, and clap.
tesseract is optional and only used for OCR.
