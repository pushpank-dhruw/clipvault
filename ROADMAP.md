# ClipVault Roadmap

## Phase 1 — Core (✅ Complete)

- [x] Rust + egui project scaffold (eframe wgpu, 7 source files)
- [x] SQLite clipboard store — CRUD, SHA-256 dedup, 500-entry auto-eviction
- [x] wl-paste clipboard monitoring (500ms polling interval)
- [x] Unix socket IPC — toggle/status/quit commands
- [x] egui floating overlay with Tokyo Night dark theme
- [x] Fuzzy search via skim matcher
- [x] clap CLI — toggle, list, search, clear, status
- [x] Hyprland integration — keybind (SUPER SHIFT V), autostart, window rules
- [x] Release build — 14MB, stripped, LTO-optimized
- [x] XDG-compliant config (`~/.config/clipvault/config.toml`) + DB (`~/.local/share/clipvault/`)
- [x] 4 passing unit tests on store module
- [x] `cargo clippy -D warnings` clean

## Phase 2 — Polish (🔄 Next)

### Phase 2A — Data Model + Image Capture (✅ Complete)

- [x] Schema migration — content_path, mime_type, category columns
- [x] Image storage — files on disk at ~/.local/share/clipvault/images/
- [x] ClipboardContent enum — Text | Image variants
- [x] MIME type detection via wl-paste --list-types
- [x] New store methods: insert_image, list_by_type, list_by_source, set_category
- [x] Category system — predefined (Code, Design, Links, Notes, Sensitive) + user CRUD
- [x] evict_old() skips favorites (text) and favorites (images)
- [x] Image paste support via wl-copy --type
- [x] 9 passing tests (4 new: image, type filter, source filter, categories)

### Phase 2B — Notch Shelf UI

- [ ] Shelf mode — 800x120 top-center bar
- [ ] Horizontal scroll of clip cards
- [ ] Text preview + image thumbnails
- [ ] Type filter tabs
- [ ] Favorites filter toggle
- [ ] Keyboard nav in shelf mode

### Phase 2C — Library View

- [ ] Full-screen library overlay (1200x800)
- [ ] Group by app with collapsible sections
- [ ] Category filter tabs
- [ ] Right-click context menu
- [ ] Full fuzzy search

### Phase 2D — Advanced

- [ ] Systemd user service for robust daemon autostart
- [ ] Tray icon — show ClipVault is running (egui or ksni)
- [ ] Waybar module — clipboard count status
- [ ] Multi-monitor position awareness for overlay
- [ ] Configurable max entries & poll interval from GUI settings

## Phase 3 — Advanced

- [ ] OCR for image clipboard content (text in screenshots)
- [ ] Snippet templates — save and insert reusable text
- [ ] Sensitive content detection & auto-filter passwords/keys
- [ ] History export/import — JSON, CSV formats
- [ ] Advanced search — by date range, source app, content type
- [ ] Encryption for selected/pinned entries
- [ ] Auto-cleanup rules — age-based, app-based eviction

## Phase 4 — Ecosystem

- [ ] Flatpak distribution for broader Linux reach
- [ ] Plugin system — custom actions on clipboard match
- [ ] Password manager integration (bitwarden, keepassxc)
- [ ] Global clipboard sync across devices (opt-in, encrypted)
- [ ] clipvault.dev domain launch and project website
- [ ] Community contribution guide and issue templates
