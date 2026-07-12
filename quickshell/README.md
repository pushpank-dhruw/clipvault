# clipvault Quickshell frontend

The shelf UI, written in QML for [Quickshell](https://quickshell.org). It talks
to the headless `clipvault` daemon over the unix socket
(`$XDG_RUNTIME_DIR/clipvault.sock`) using a newline-delimited JSON protocol.

## Files

| File | Role |
|------|------|
| `shell.qml`   | Root. Owns the daemon `Socket`, the entry model, command helpers, hover-open state, and an `IpcHandler` (`shelf`). |
| `Shelf.qml`   | The notch: a top-center `wlr-layer-shell` `PanelWindow`. Header (tabs + search) and a horizontal card list. |
| `Card.qml`    | One clipboard entry: preview, source, favorite/OCR/delete, click-to-paste. |
| `HotZone.qml` | Invisible top-center strip that opens the shelf on hover-dwell. |

## Run (development)

The daemon must be running first:

```fish
clipvault                 # headless daemon (or: systemctl --user start clipvault)
```

Then launch the frontend against this directory:

```fish
qs -p ./shell.qml         # run this config directly
# or, once installed to ~/.config/quickshell/clipvault or /etc/xdg/quickshell/clipvault:
qs -c clipvault
```

Toggle the shelf:

```fish
clipvault toggle                       # via the daemon (pushes a toggle event)
qs -c clipvault ipc call shelf toggle  # directly to the frontend
```

## Install

Symlink for live editing:

```fish
ln -s (pwd) ~/.config/quickshell/clipvault
```

The Arch package installs these to `/etc/xdg/quickshell/clipvault/`.

## IPC protocol (what the daemon speaks)

Requests are `{"id":N,"cmd":"…","args":{…}}\n`; responses `{"id":N,"ok":true,"data":…}\n`.
Send `{"cmd":"subscribe"}` to receive unsolicited `{"event":"changed"}` (history
changed) and `{"event":"toggle"}` (from `clipvault toggle`).

Commands: `list` (`filter`: all/text/image/favorites, `category`, `limit`, `offset`),
`search` (`query`), `counts`, `categories`, `entry` (`id`), `favorite` (`id`),
`set_category` (`id`, `category`), `delete` (`id`), `paste` (`id`), `ocr` (`id`),
`clear`, `toggle`, `status`.

Image entries carry `content_path` and `thumb_path` (on-disk) — render with
`Image { source: "file://" + thumb_path }`.
