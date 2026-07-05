# ClipVault Roadmap

> Direction informed by competitive research of [Supaste](https://www.supaste.com/)
> (macOS clipboard manager): notch shelf + library window, type/app/category
> organization, OCR, multi-clip paste, snippets, color clips, screen text capture.
> ClipVault targets feature parity on Linux/Wayland with native tooling.

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

## Phase 2 — Shelf (✅ Complete)

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

### Phase 2B — Notch Shelf UI (✅ Complete)

- [x] Shelf mode: 800x140 top-center bar, frameless, rounded, drop shadow
- [x] Horizontal scroll of clip cards
- [x] Text preview + image thumbnails (cached textures)
- [x] Type filter tabs (All / Text / Image)
- [x] Favorites filter toggle
- [x] Keyboard nav in shelf mode
- [x] Inline search in shelf
- [x] IPC: proper quit command, status now returns entry count
- [x] CLI simplified: toggle, quit, list, search, clear, status

> Architecture note: an interim Overlay / Shelf / Library multi-view system (ViewMode)
> was built, then removed in favor of a single shelf view. Library-style management
> (context menu, categories, favorites, delete) lives directly in the shelf.

### Phase 2C — Shelf Refinements (✅ Complete)

- [x] Right-click context menu with Favorite toggle, Category assignment (inline pills), Delete
- [x] Delete confirmation dialog with Cancel/Delete buttons
- [x] Favorite star indicator on shelf cards
- [x] Category badges on shelf cards
- [x] Hover effects: border highlight + action buttons (favorite, delete)
- [x] Favorites filter pill in shelf filter row
- [x] Context menu positioning fixed relative to card (right edge)
- [x] Escape key closes context menu and delete dialog

## Phase 3 — Ship v0.1 to Omarchy (AUR) (🔄 Next)

Distribution mandate: Omarchy / Arch Linux only for now, via a versioned AUR package.

### 3A — Wayland windowing fix (pre-publish gate)

- [ ] Invisible root host viewport with app_id `clipvault`
- [ ] Shelf as child viewport with app_id `clipvault-shelf`; hide by not rendering the viewport (real unmap on Wayland, fixes toggle-off and Escape)
- [ ] Hyprland windowrules can now match real classes
- [ ] Config forward-compat: `#[serde(default)]` so old config.toml files keep working

### 3B — v0.1.0 release + AUR package

- [ ] `packaging/PKGBUILD`: versioned build from GitHub release tarball, cargo --locked
- [ ] `packaging/clipvault.service`: systemd user unit (robust autostart, replaces exec-once)
- [ ] `packaging/hyprland-clipvault.conf`: SUPER+SHIFT+V bind + float/positioning windowrules
- [ ] README Install section: `yay -S clipvault`, `systemctl --user enable --now clipvault`
- [ ] Tag v0.1.0, GitHub release, `.SRCINFO`, AUR repo push

### 3C — Omarchy onboarding

- [ ] Omarchy-specific docs: drop the bind into `~/.config/hypr/bindings.conf`
- [ ] Tokyo Night default theme matches Omarchy aesthetic out of the box

## Phase 4 — Library Window

The full-window counterpart to the shelf (Supaste's "Clipboard Library": masonry grid,
detail overlay, context menu).

- [ ] Library window (second viewport, app_id `clipvault-library`): card grid with thumbnails, type badges, time-ago + size labels
- [ ] Tab/filter row with live counts (History / Favorites / per-category; needs `count_favorites()`)
- [ ] Detail side panel: large preview, created-at, source app, size, type, category
- [ ] Detail panel actions: copy, favorite, categorize, delete (reuse shelf context-menu patterns)
- [ ] Source-app normalization: empty `hyprctl` class stored as NULL, never ""
- [ ] Source-app name rendered on cards, detail panel, and shelf tooltips
- [ ] Search across content, category, and source app (extend `Store::search` SQL)
- [ ] Source filter dropdown fed by `list_sources()`
- [ ] Shelf to Library handoff (expand button on shelf opens Library)
- [ ] `clipvault library` CLI + IPC command
- [ ] Multi-monitor position awareness for shelf and library

## Phase 5 — Organization & Types

- [ ] Custom categories: create/rename/color from the UI with inline "+" pill (store CRUD already exists)
- [ ] Category tabs show live counts (Supaste: History 24, Colors 24, ...)
- [ ] First-class type detection from text content: link, email, code, color, address
- [ ] Color clips rendered as swatch cards (like Supaste's #0080FF card)
- [ ] Find by app + find by type filter rows
- [ ] Collection views: List / Card / Board (Kanban columns) in Library
- [ ] Advanced search: date range, source app, content type combined
- [ ] History export/import: JSON, CSV formats

## Phase 6 — Paste Power

- [ ] Quick Paste popup: centered Spotlight-style picker (search + type pills + card row), Enter pastes into focused app via `wtype`/`ydotool`
- [ ] Last-N hotkey paste: `clipvault paste --recent N` CLI + Hyprland binds SUPER+SHIFT+0-9
- [ ] Multi-clip copy: collect several clips into one combined card, batch-paste
- [ ] Snippet templates: reusable text with `{placeholders}` (Supaste "email templates")
- [ ] Paste transforms: plain-text, trim, case conversion
- [ ] STRETCH: inline `;shortcut` expansion in any app (needs evdev/uinput or IME; technical spike first)

## Phase 7 — Capture Superpowers

Wayland-native equivalents of Supaste v1.3 capture features.

- [ ] OCR on image clips: "Copy text" action (`ocrs` crate or tesseract), OCR text searchable
- [ ] Capture text from screen: `grim` + `slurp` region, then OCR, saved as text clip
- [ ] Color picker: `hyprpicker` output saved as color clip with rendered swatch
- [ ] Screenshot auto-ingest: watch screenshots directory, offer to save as clip

## Phase 8 — Retention, Safety & Desktop Polish

- [ ] Clip reminders: quick-time chips, custom date, on-app-return trigger; `notify-send` + daemon scheduler
- [ ] Sensitive content detection: auto-filter passwords/keys from capture
- [ ] Settings UI: retention rules per type, theme, shortcuts
- [ ] Auto-cleanup rules: age-based, app-based eviction
- [ ] Encryption for selected/pinned entries
- [ ] Tray icon (ksni) + Waybar module (clipboard count)

## Phase 9 — Ecosystem

- [ ] Flatpak distribution (after Arch/AUR is solid)
- [ ] Plugin system: custom actions on clipboard match
- [ ] Password manager integration (bitwarden, keepassxc)
- [ ] Global clipboard sync across devices (opt-in, encrypted)
- [ ] clipvault.dev domain launch and project website
- [ ] Community contribution guide and issue templates

---

**Per-phase verification:** `cargo test` green, `cargo clippy -- -D warnings` clean,
manual smoke test (daemon + toggle + feature under test), signed commit per phase.
