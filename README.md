# ClipVault

Clipboard history for Hyprland. It lives at the top of the screen and drops
down when you reach for it, so you can grab something you copied earlier
without losing your place.

I built it for my own Omarchy machine, so it leans hard on Hyprland and
Wayland. That's where it works.

## What it does

Hover the top center of the screen (or press your keybind) and the shelf
drops down. It keeps everything you copy, text and images, and lays it out as
cards you scroll through. Click a card to copy it back.

A few things worth knowing:

- Text and image clips, with image thumbnails rendered right on the card.
- Tabs for All / Text / Images / Favorites, plus search when the list gets long.
- The OCR button pulls text out of a screenshot, if you have tesseract.
- Each card shows the app you copied from; hover to favorite or delete it.
- A ⚙ settings window to tune the look and behaviour, applied live.

## Architecture

Two pieces talking over a unix socket:

- **`clipvault`** — a small headless Rust daemon: watches the Wayland
  clipboard, stores history in SQLite, and serves a JSON IPC protocol.
- **A Quickshell (QML) frontend** — the shelf itself, a real `wlr-layer-shell`
  notch. It subscribes to the daemon and renders the cards.

The daemon has no GUI of its own, so it is light and starts instantly.

## Install

### Arch / Omarchy

```bash
yay -S clipvault                 # pulls in quickshell + wl-clipboard
systemctl --user enable --now clipvault
echo 'source = /usr/share/clipvault/hyprland-clipvault.conf' >> ~/.config/hypr/hyprland.conf
hyprctl reload
```

The sourced config launches the frontend (`exec-once = qs -c clipvault`) and
binds `SUPER+SHIFT+V`. On Omarchy the bind fits in `~/.config/hypr/bindings.conf`
if you keep your keybinds together.

### From source

```bash
make install         # builds + installs the binary, QML, and the systemd unit
systemctl --user enable --now clipvault
```

Then add to `~/.config/hypr/hyprland.conf`:

```ini
exec-once = qs -c clipvault
bind = SUPER SHIFT, V, exec, clipvault toggle
```

For development, run the pieces directly instead of installing:

```bash
cargo run &                                     # daemon
ln -s "$PWD/quickshell" ~/.config/quickshell/clipvault
qs -c clipvault                                 # frontend
```

Needs [`quickshell`](https://quickshell.org) for the UI, `wl-clipboard` for
capture, and (optionally) `tesseract` for OCR.

## Commands

| Command | What it does |
|---|---|
| `clipvault open` | Show the shelf, starting the daemon + frontend if needed |
| `clipvault` | Start the headless daemon (capture + IPC) |
| `clipvault toggle` | Show or hide the shelf (signals the frontend) |
| `clipvault quit` | Stop the daemon |
| `clipvault list` | Print history as JSON |
| `clipvault search <query>` | Search from the terminal |
| `clipvault clear` | Wipe the history |
| `clipvault status` | Daemon status and entry count |

You can also toggle the frontend directly with
`qs -c clipvault ipc call shelf toggle`, and open settings with
`qs -c clipvault ipc call shelf settings`.

### Opening it

Three ways to open the shelf:

- **Hover** the top-center notch (the shelf drops down).
- **Keybind** `SUPER+SHIFT+V`.
- **App launcher / panel** — the package installs a `Clipvault` entry
  (`clipvault open`) so it shows up in your app launcher. For a button on your
  bar, merge `packaging/waybar-clipvault.jsonc` into your Waybar config.

`clipvault open` starts the daemon and the Quickshell frontend if they are not
already running, so it works as a cold-start entry point.

## Hyprland

The package ships a ready config at
`/usr/share/clipvault/hyprland-clipvault.conf`. The shelf and hover hot-zone
are layer-shell surfaces that place themselves, so there are no window rules
for them. The only toplevel is the settings dialog:

```ini
exec-once = qs -c clipvault
bind = SUPER SHIFT, V, exec, clipvault toggle

# settings dialog (Quickshell FloatingWindow)
windowrule = float on, match:class ^(org.quickshell)$
windowrule = size 480 620, match:class ^(org.quickshell)$
windowrule = opacity 1.0 override 1.0 override, match:class ^(org.quickshell)$
```

## Config

Lives at `~/.config/clipvault/config.toml`. The ⚙ settings window edits it for
you and applies changes live; you can also edit it by hand. The defaults are
fine:

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
notch_hover_width = 300.0         # hot-zone width, centered at the top edge
notch_hover_dwell_ms = 120        # how long to dwell before it opens
notch_hover_close_delay_ms = 400  # grace period before a hover-opened shelf hides
ocr_enabled = false
hide_sensitive = false            # skip clips marked sensitive (password managers)
image_store_dir = "images"
```

## Built with

Rust (rusqlite, wl-clipboard-rs, tokio, clap) for the daemon, and Quickshell
(QML / QtQuick, wlr-layer-shell) for the frontend. tesseract is optional and
only used for OCR.
