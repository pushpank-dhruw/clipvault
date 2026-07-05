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

- [ ] Systemd user service for robust daemon autostart
- [ ] Tray icon — show ClipVault is running (egui or ksni)
- [ ] Image clipboard preview in overlay
- [ ] Waybar module — clipboard count status
- [ ] Multi-monitor position awareness for overlay
- [ ] Paste-on-select mode — click to paste without closing
- [ ] Configurable max entries & poll interval from GUI settings
- [ ] Keyboard navigation in overlay (arrow keys, enter to paste)

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
