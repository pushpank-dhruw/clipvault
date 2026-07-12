# ClipVault (Omarchy Clipboard Manager)

Clipboard history for Hyprland, split into a headless Rust daemon and a
Quickshell (QML) frontend that talk over a unix-socket JSON protocol.

## Tech Stack

- **Daemon**: Rust (edition 2024) — headless, no GUI toolkit
  - **Clipboard**: wl-clipboard-rs (Wayland)
  - **Storage**: rusqlite (SQLite, bundled)
  - **IPC**: interprocess (Unix domain socket) + serde_json line protocol
  - **Async**: tokio (clipboard poll loop)
  - **CLI**: clap; **Config**: toml + serde; **Errors**: anyhow + thiserror;
    **Logging**: tracing
- **Frontend**: Quickshell (QML / QtQuick, React-Aria-free), `wlr-layer-shell`
  - Lives in `quickshell/`, run with `qs -c clipvault`

## Commands

```bash
cargo build --release           # Release build (LTO, strip)
cargo check                     # Fast check (dev cycle)
cargo test                      # All tests (incl. a real-socket IPC test)
cargo clippy --all-targets --locked -- -D warnings
cargo fmt                       # Format (must pass before commit)

cargo run                       # Run the headless daemon
cargo run -- toggle             # Signal the frontend to toggle the shelf
cargo run -- list               # Print history as JSON
cargo run -- search <query>     # Search from terminal
cargo run -- clear              # Clear history
cargo run -- status             # Daemon status + count

# Frontend (needs a Wayland session)
ln -s "$PWD/quickshell" ~/.config/quickshell/clipvault
qs -c clipvault                 # run the shelf
qmllint -I /usr/lib/qt6/qml quickshell/*.qml   # static-check the QML
```

## Code Style

- Follow `rustfmt` defaults; `cargo clippy` must pass with `-D warnings`.
- Never use `unwrap()`/`expect()` outside tests.
- `anyhow::Result` for the binary, `thiserror` for reusable errors.
- Library crate + binary: `src/lib.rs` re-exports modules; `src/main.rs` is a
  thin CLI + daemon lifecycle.
- Public items documented with `///`. Tests named `should_*` / behaviour-first.

## Architecture

```
src/                      # the daemon (headless)
├── main.rs     # CLI (clap) + daemon lifecycle: spawns monitor + IPC threads
├── lib.rs      # public API, module re-exports
├── monitor.rs  # Wayland clipboard capture (wl-clipboard-rs); reads poll
│               #   interval + hide_sensitive from shared config each tick
├── store.rs    # SQLite CRUD (dedup by SHA-256, eviction, search, thumbnails)
├── ipc.rs      # unix socket: JSON line protocol + subscribe/push events
├── ocr.rs      # OCR of image entries via the `tesseract` CLI
├── icons.rs    # freedesktop app-icon resolution (currently unused by the UI)
└── config.rs   # ~/.config/clipvault/config.toml (serde)

quickshell/               # the frontend (QML)
├── shell.qml   # root: two daemon sockets (request + event), model, config,
│               #   IpcHandler (`shelf`: toggle/show/hide/settings)
├── Shelf.qml   # the notch: a top-center wlr-layer-shell PanelWindow
├── Card.qml    # one entry: preview / thumbnail, source, favorite/OCR/delete
├── HotZone.qml # invisible top strip that opens the shelf on hover-dwell
└── Settings.qml# FloatingWindow form → set_config (live config editor)
```

There is **no GUI in the daemon**. The old egui shelf (`gui.rs`) and Rust
cursor-polling hover watcher (`hover.rs`) were removed; the frontend is entirely
Quickshell, and hover detection is a QML `HoverHandler` on `HotZone`.

## IPC protocol (`ipc.rs`)

Newline-delimited JSON over `/run/user/$UID/clipvault.sock`.

- Request: `{"id":N,"cmd":"…","args":{…}}` → Response: `{"id":N,"ok":true,"data":…}`
- A client sends `{"cmd":"subscribe"}` on a **separate** connection to receive
  unsolicited events: `{"event":"changed"}` (history changed),
  `{"event":"toggle"}` (from `clipvault toggle`), `{"event":"config"}` (config
  changed). A subscribed connection becomes event-only, so commands need their
  own connection — the frontend opens two sockets.
- Commands: `list` (`filter` all/text/image/favorites, `category`, `limit`,
  `offset`), `search`, `counts`, `categories`, `entry`, `favorite`,
  `set_category`, `delete`, `paste`, `ocr`, `clear`, `config`, `set_config`,
  `reload`, `toggle`, `status`.
- Legacy raw-token commands (`toggle`/`quit`/`status`, no newline) still work
  for the CLI.

## Data Flow

```
Wayland clipboard changes
  → monitor.rs captures text/image + source app + timestamp
  → store.rs inserts into SQLite (dedup by SHA-256; image + thumbnail to disk)
  → daemon broadcasts {"event":"changed"} to subscribers
  → Quickshell frontend re-queries `list`/`counts` and re-renders the cards
  → user clicks a card → `paste` → daemon copies it back via wl-copy
```

## Data Locations

| Purpose | Path |
|---|---|
| Config | `~/.config/clipvault/config.toml` |
| Database | `~/.local/share/clipvault/clipvault.db` |
| Images + thumbnails | `~/.local/share/clipvault/images/` (`thumb_*`) |
| IPC socket | `/run/user/$UID/clipvault.sock` |
| Frontend config | `~/.config/quickshell/clipvault/` (or `/etc/xdg/…`) |

## Design (Tokyo Night)

- **bg** `#1a1b26`, **surface** `#24283b`, **accent** `#7aa2f7`,
  **fg** `#c0caf5`, **muted** `#565f89`, favorite `#e0af68`, delete `#f7768e`.
- Shelf: rounded top-center layer-shell notch, subtle drop-in/fade animation.
