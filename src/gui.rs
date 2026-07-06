use crate::config::Config;
use crate::store::{ClipboardEntry, Store};
use crate::{icons, ocr};
use eframe::egui;
use egui::{Color32, CornerRadius, FontFamily, FontId, Key, TextureHandle, Vec2};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const BG: Color32 = Color32::from_rgb(0x1a, 0x1b, 0x26);
const FG: Color32 = Color32::from_rgb(0xc0, 0xca, 0xf5);
const FG_MUTED: Color32 = Color32::from_rgb(0xa9, 0xb1, 0xd6);
const FG_DIM: Color32 = Color32::from_rgb(0x56, 0x5f, 0x89);
const RED: Color32 = Color32::from_rgb(0xf7, 0x76, 0x8e);
const GREEN: Color32 = Color32::from_rgb(0x9e, 0xce, 0x6a);
const YELLOW: Color32 = Color32::from_rgb(0xe0, 0xaf, 0x68);
const ACCENT: Color32 = Color32::from_rgb(0x7a, 0xa2, 0xf7);
const ENTRY_BG: Color32 = Color32::from_rgb(0x1e, 0x20, 0x30);
const ENTRY_BG_SELECTED: Color32 = Color32::from_rgb(0x28, 0x2c, 0x42);
const IMG_BG: Color32 = Color32::from_rgb(0x16, 0x17, 0x20);
const BORDER: Color32 = Color32::from_rgb(0x2a, 0x2e, 0x42);
const BORDER_HOVER: Color32 = Color32::from_rgb(0x41, 0x48, 0x6b);
const OVERLAY_BTN_BG: Color32 = Color32::from_rgb(0x10, 0x11, 0x18);
const SEARCH_BG: Color32 = Color32::from_rgb(0x14, 0x15, 0x1e);
const BTN_BG: Color32 = Color32::from_rgb(0x24, 0x27, 0x3a);
const TAB_ACTIVE_BG: Color32 = Color32::from_rgb(0xed, 0xf0, 0xfc);
const TAB_ACTIVE_FG: Color32 = Color32::from_rgb(0x1a, 0x1b, 0x26);

/// Thumbnails are decoded at ~2x the on-screen card size so images stay crisp.
const THUMB_PX: u32 = 240;

/// How long the "Copied" badge stays on a card after a paste.
const COPIED_BADGE: Duration = Duration::from_millis(1200);
/// How long an OCR status line lingers.
const OCR_STATUS: Duration = Duration::from_millis(2600);

pub struct ClipboardApp {
    store: Arc<Mutex<Store>>,
    entries: Vec<ClipboardEntry>,
    search_query: String,
    selected_index: Option<usize>,
    active_filter: Option<String>,
    active_category: Option<String>,
    categories: Vec<(i64, String, Option<String>)>,
    thumbnails: HashMap<i64, TextureHandle>,
    /// Cached app-logo textures keyed by window class; `None` = no icon found.
    app_icons: HashMap<String, Option<TextureHandle>>,
    /// Cached on-disk byte size of image entries, keyed by id.
    image_sizes: HashMap<i64, u64>,
    /// (total, text, image, favorites) counts shown on the tabs.
    counts: (usize, usize, usize, usize),
    shelf_loaded: bool,
    context_menu_entry_id: Option<i64>,
    context_menu_visible: bool,
    context_menu_card_rect: Option<egui::Rect>,
    delete_confirm_entry_id: Option<i64>,
    delete_confirm_visible: bool,
    hovered_entry_id: Option<i64>,
    /// Last image card the pointer hovered; the OCR button targets this.
    last_image_hovered: Option<i64>,
    scroll_to_selected: bool,
    /// When set, the shelf hides once this instant is `COPIED_BADGE`-old, so
    /// the "Copied" confirmation is visible before the shelf closes.
    pending_hide: Option<Instant>,
    page_size: usize,
    ocr_available: bool,
    /// Set while an OCR job runs; the worker thread fills the inner slot.
    ocr_pending: Option<Arc<Mutex<Option<String>>>>,
    /// Transient status line (message + shown-at) for OCR feedback.
    ocr_status: Option<(String, Instant)>,
    /// (entry id, copied-at) driving the transient "Copied" badge.
    copied: Option<(i64, Instant)>,
    pub should_hide: bool,
}

impl ClipboardApp {
    pub fn new(store: Arc<Mutex<Store>>, config: &Config) -> Self {
        let page_size = config.shelf_max_entries.max(1);
        let (categories, counts) = {
            let s = store.lock().unwrap();
            (
                s.list_categories().unwrap_or_default(),
                s.type_counts().unwrap_or_default(),
            )
        };
        let entries = fetch_entries(&store, None, None, page_size, 0);
        let selected_index = if entries.is_empty() { None } else { Some(0) };
        Self {
            store,
            entries,
            search_query: String::new(),
            selected_index,
            active_filter: None,
            active_category: None,
            categories,
            thumbnails: HashMap::new(),
            app_icons: HashMap::new(),
            image_sizes: HashMap::new(),
            counts,
            shelf_loaded: false,
            context_menu_entry_id: None,
            context_menu_visible: false,
            context_menu_card_rect: None,
            delete_confirm_entry_id: None,
            delete_confirm_visible: false,
            hovered_entry_id: None,
            last_image_hovered: None,
            scroll_to_selected: false,
            pending_hide: None,
            page_size,
            ocr_available: ocr::available(),
            ocr_pending: None,
            ocr_status: None,
            copied: None,
            should_hide: false,
        }
    }

    fn refresh_entries(&mut self) {
        let limit = self.page_size;
        let offset = 0;

        if !self.search_query.is_empty() {
            self.entries = self
                .store
                .lock()
                .unwrap()
                .search(&self.search_query, limit)
                .unwrap_or_default();
        } else if let Some(ref ft) = self.active_filter {
            match ft.as_str() {
                "favorites" => {
                    self.entries = self
                        .store
                        .lock()
                        .unwrap()
                        .list_favorites(limit, offset)
                        .unwrap_or_default();
                }
                _ => {
                    if let Some(ref cat) = self.active_category {
                        self.entries = self
                            .store
                            .lock()
                            .unwrap()
                            .list_by_type_and_category(ft, cat, limit, offset)
                            .unwrap_or_default();
                    } else {
                        self.entries = self
                            .store
                            .lock()
                            .unwrap()
                            .list_by_type(ft, limit, offset)
                            .unwrap_or_default();
                    }
                }
            }
        } else if let Some(ref cat) = self.active_category {
            self.entries = self
                .store
                .lock()
                .unwrap()
                .list_by_category(cat, limit, offset)
                .unwrap_or_default();
        } else {
            self.entries = self
                .store
                .lock()
                .unwrap()
                .list(limit, offset)
                .unwrap_or_default();
        }

        self.selected_index = if self.entries.is_empty() {
            None
        } else {
            Some(0)
        };
        self.counts = self.store.lock().unwrap().type_counts().unwrap_or_default();
    }

    fn load_thumbnail(&mut self, ctx: &egui::Context, id: i64, _entry: &ClipboardEntry) {
        if self.thumbnails.contains_key(&id) {
            return;
        }
        let data = self
            .store
            .lock()
            .unwrap()
            .get_image_thumbnail(id)
            .ok()
            .flatten();
        let data = data.or_else(|| self.store.lock().unwrap().get_image_data(id).ok().flatten());
        if let Some(bytes) = data
            && let Ok(img) = image::load_from_memory(&bytes)
        {
            let thumb = img.thumbnail(THUMB_PX, THUMB_PX);
            let rgba = thumb.to_rgba8();
            let (w, h) = rgba.dimensions();
            if w == 0 || h == 0 {
                return;
            }
            let pixels = rgba.into_raw();
            let color_image =
                egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], &pixels);
            let tex = ctx.load_texture(
                format!("thumb_{}", id),
                color_image,
                egui::TextureOptions::default(),
            );
            self.thumbnails.insert(id, tex);
        }
    }
}

impl ClipboardApp {
    fn handle_keyboard_shelf(&mut self, ctx: &egui::Context) -> bool {
        // Only act on navigation keys when the shelf actually holds keyboard
        // focus. A hover-opened shelf is mapped without focus, so a stray
        // Enter can never silently overwrite the clipboard here.
        let focused = ctx.input(|i| i.focused);
        if !focused || ctx.wants_keyboard_input() {
            return false;
        }

        if ctx.input(|i| i.key_pressed(Key::ArrowLeft)) {
            let new_idx = self.selected_index.map_or(0, |i| i.saturating_sub(1));
            self.selected_index = Some(new_idx);
            self.scroll_to_selected = true;
            return true;
        }
        if ctx.input(|i| i.key_pressed(Key::ArrowRight)) {
            let max = self.entries.len().saturating_sub(1);
            let new_idx = self
                .selected_index
                .map_or(0, |i| i.saturating_add(1).min(max));
            self.selected_index = Some(new_idx);
            self.scroll_to_selected = true;
            return true;
        }
        if ctx.input(|i| i.key_pressed(Key::Enter))
            && let Some(idx) = self.selected_index
            && let Some(entry) = self.entries.get(idx)
        {
            paste_entry(entry, &self.store);
            self.should_hide = true;
            return true;
        }
        false
    }
}

enum MenuAction {
    ToggleFavorite,
    SetCategory(String),
    ClearCategory,
    Delete,
}

impl ClipboardApp {
    fn render_context_menu(&mut self, ctx: &egui::Context) {
        if !self.context_menu_visible {
            return;
        }

        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.context_menu_visible = false;
            self.context_menu_entry_id = None;
            return;
        }

        let entry_id = match self.context_menu_entry_id {
            Some(id) => id,
            None => return,
        };

        let is_favorite = self
            .entries
            .iter()
            .find(|e| e.id == entry_id)
            .map(|e| e.favorite)
            .unwrap_or(false);
        let has_category = self
            .entries
            .iter()
            .find(|e| e.id == entry_id)
            .and_then(|e| e.category.as_deref())
            .map(|s| s.to_owned());
        let categories = self.categories.clone();

        let card_rect = match self.context_menu_card_rect {
            Some(r) => r,
            None => return,
        };

        let pos = egui::pos2(card_rect.right() + 4.0, card_rect.top());

        let area = egui::Area::new(egui::Id::new("clipvault_context_menu"))
            .fixed_pos(pos)
            .order(egui::Order::Foreground);

        area.show(ctx, |ui| {
            let frame = egui::Frame {
                fill: ENTRY_BG,
                corner_radius: CornerRadius::same(8),
                stroke: egui::Stroke::new(1.0, Color32::from_rgb(0x31, 0x34, 0x4a)),
                shadow: egui::epaint::Shadow {
                    offset: [0, 4],
                    blur: 16,
                    spread: 0,
                    color: Color32::from_black_alpha(80),
                },
                ..Default::default()
            };

            let menu_result = frame.show(ui, |ui| {
                ui.set_min_width(160.0);

                let star = if is_favorite {
                    "★ Favorite"
                } else {
                    "☆ Favorite"
                };
                let star_color = if is_favorite { GREEN } else { FG };
                if ui
                    .add(
                        egui::Button::new(
                            egui::RichText::new(star)
                                .font(FontId {
                                    size: 12.0,
                                    family: FontFamily::Proportional,
                                })
                                .color(star_color),
                        )
                        .fill(Color32::TRANSPARENT)
                        .min_size(Vec2::new(160.0, 24.0)),
                    )
                    .clicked()
                {
                    return Some(MenuAction::ToggleFavorite);
                }

                ui.separator();

                ui.add_space(2.0);
                ui.label(
                    egui::RichText::new("Category")
                        .font(FontId {
                            size: 10.0,
                            family: FontFamily::Monospace,
                        })
                        .color(FG_DIM),
                );
                ui.add_space(4.0);

                let mut cat_action: Option<MenuAction> = None;
                ui.horizontal_wrapped(|ui| {
                    let cat_bg = Color32::from_rgb(0x24, 0x27, 0x3a);
                    for cat in &categories {
                        let is_active = has_category.as_deref() == Some(&cat.1);
                        let fill = if is_active { ACCENT } else { cat_bg };
                        if ui
                            .add(
                                egui::Button::new(
                                    egui::RichText::new(&cat.1)
                                        .font(FontId {
                                            size: 10.0,
                                            family: FontFamily::Monospace,
                                        })
                                        .color(if is_active { Color32::WHITE } else { FG }),
                                )
                                .fill(fill)
                                .corner_radius(10)
                                .min_size(Vec2::new(4.0, 20.0)),
                            )
                            .clicked()
                        {
                            cat_action = Some(MenuAction::SetCategory(cat.1.clone()));
                        }
                        ui.add_space(2.0);
                    }

                    if has_category.is_some() {
                        ui.add_space(2.0);
                        if ui
                            .add(
                                egui::Button::new(
                                    egui::RichText::new("×")
                                        .font(FontId {
                                            size: 10.0,
                                            family: FontFamily::Monospace,
                                        })
                                        .color(RED),
                                )
                                .fill(cat_bg)
                                .corner_radius(10)
                                .min_size(Vec2::new(20.0, 20.0)),
                            )
                            .clicked()
                        {
                            cat_action = Some(MenuAction::ClearCategory);
                        }
                    }
                });
                if let Some(action) = cat_action {
                    return Some(action);
                }

                ui.add_space(4.0);
                ui.separator();

                ui.add_space(2.0);
                if ui
                    .add(
                        egui::Button::new(
                            egui::RichText::new("Delete")
                                .font(FontId {
                                    size: 12.0,
                                    family: FontFamily::Proportional,
                                })
                                .color(RED),
                        )
                        .fill(Color32::TRANSPARENT)
                        .min_size(Vec2::new(160.0, 24.0)),
                    )
                    .clicked()
                {
                    return Some(MenuAction::Delete);
                }

                None
            });

            if let Some(action) = menu_result.inner {
                match action {
                    MenuAction::ToggleFavorite => {
                        self.store.lock().unwrap().toggle_favorite(entry_id).ok();
                    }
                    MenuAction::SetCategory(cat) => {
                        self.store
                            .lock()
                            .unwrap()
                            .set_category(entry_id, Some(&cat))
                            .ok();
                    }
                    MenuAction::ClearCategory => {
                        self.store.lock().unwrap().set_category(entry_id, None).ok();
                    }
                    MenuAction::Delete => {
                        self.delete_confirm_entry_id = Some(entry_id);
                        self.delete_confirm_visible = true;
                    }
                }
                self.refresh_entries();
                self.context_menu_visible = false;
                self.context_menu_entry_id = None;
            }
        });
    }

    fn render_delete_dialog(&mut self, ctx: &egui::Context) {
        if !self.delete_confirm_visible {
            return;
        }

        egui::Window::new("Confirm Delete")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .title_bar(false)
            .frame(egui::Frame {
                fill: ENTRY_BG,
                corner_radius: CornerRadius::same(12),
                stroke: egui::Stroke::new(1.0, Color32::from_rgb(0x31, 0x34, 0x4a)),
                ..Default::default()
            })
            .show(ctx, |ui| {
                ui.set_min_width(200.0);
                ui.label(
                    egui::RichText::new("Delete this entry?")
                        .font(FontId {
                            size: 13.0,
                            family: FontFamily::Proportional,
                        })
                        .color(FG),
                );
                ui.add_space(4.0);
                ui.label(
                    egui::RichText::new("This will permanently remove it.")
                        .font(FontId {
                            size: 11.0,
                            family: FontFamily::Monospace,
                        })
                        .color(FG_DIM),
                );
                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    ui.add_space(4.0);
                    if ui
                        .add(
                            egui::Button::new(
                                egui::RichText::new("Cancel")
                                    .font(FontId {
                                        size: 11.0,
                                        family: FontFamily::Monospace,
                                    })
                                    .color(FG),
                            )
                            .fill(Color32::from_rgb(0x24, 0x27, 0x3a))
                            .corner_radius(6)
                            .min_size(Vec2::new(80.0, 28.0)),
                        )
                        .clicked()
                    {
                        self.delete_confirm_visible = false;
                        self.delete_confirm_entry_id = None;
                    }
                    ui.add_space(8.0);
                    if ui
                        .add(
                            egui::Button::new(
                                egui::RichText::new("Delete")
                                    .font(FontId {
                                        size: 11.0,
                                        family: FontFamily::Monospace,
                                    })
                                    .color(Color32::WHITE),
                            )
                            .fill(RED)
                            .corner_radius(6)
                            .min_size(Vec2::new(80.0, 28.0)),
                        )
                        .clicked()
                    {
                        if let Some(id) = self.delete_confirm_entry_id {
                            self.store.lock().unwrap().delete(id).ok();
                            self.refresh_entries();
                        }
                        self.delete_confirm_visible = false;
                        self.delete_confirm_entry_id = None;
                    }
                    ui.add_space(4.0);
                });
                ui.add_space(4.0);
            });
    }
}

impl ClipboardApp {
    /// True while a context menu or delete dialog is open (Escape should close
    /// those instead of hiding the shelf).
    pub fn modal_open(&self) -> bool {
        self.context_menu_visible || self.delete_confirm_visible
    }

    /// Render one frame of the shelf UI into the current viewport.
    pub fn ui(&mut self, ctx: &egui::Context) {
        self.should_hide = false;
        self.poll_ocr();

        ctx.style_mut(|style| {
            style.visuals = egui::Visuals {
                dark_mode: true,
                override_text_color: Some(FG),
                window_corner_radius: CornerRadius::same(12),
                window_fill: BG,
                extreme_bg_color: BG,
                code_bg_color: Color32::from_rgb(0x14, 0x15, 0x1e),
                faint_bg_color: Color32::from_rgb(0x1e, 0x20, 0x30),
                widgets: egui::style::Widgets {
                    noninteractive: egui::style::WidgetVisuals {
                        bg_fill: Color32::from_rgb(0x14, 0x15, 0x1e),
                        weak_bg_fill: Color32::from_rgb(0x14, 0x15, 0x1e),
                        bg_stroke: egui::Stroke::new(1.0, Color32::from_rgb(0x31, 0x34, 0x4a)),
                        fg_stroke: egui::Stroke::new(1.0, FG_DIM),
                        corner_radius: CornerRadius::same(8),
                        expansion: 0.0,
                    },
                    ..Default::default()
                },
                ..Default::default()
            };
        });

        self.render_shelf(ctx);

        self.render_context_menu(ctx);
        self.render_delete_dialog(ctx);

        let escape = ctx.input(|i| i.key_pressed(egui::Key::Escape));
        if self.context_menu_visible && escape {
            self.context_menu_visible = false;
            self.context_menu_entry_id = None;
        }
        if self.delete_confirm_visible && escape {
            self.delete_confirm_visible = false;
            self.delete_confirm_entry_id = None;
        }

        // Close shortly after a copy so the "Copied" badge is seen first.
        if self
            .pending_hide
            .is_some_and(|t| t.elapsed() > COPIED_BADGE)
        {
            self.pending_hide = None;
            self.should_hide = true;
        }

        // Expire the transient "Copied" and OCR status badges, and keep the
        // frame ticking while either is showing so they fade on time.
        if self.copied.is_some_and(|(_, t)| t.elapsed() > COPIED_BADGE) {
            self.copied = None;
        }
        if self
            .ocr_status
            .as_ref()
            .is_some_and(|(_, t)| t.elapsed() > OCR_STATUS)
        {
            self.ocr_status = None;
        }
        let busy = self.copied.is_some()
            || self.ocr_status.is_some()
            || self.ocr_pending.is_some()
            || self.pending_hide.is_some();
        ctx.request_repaint_after(Duration::from_millis(if busy { 60 } else { 500 }));
    }
}

impl ClipboardApp {
    fn render_shelf(&mut self, ctx: &egui::Context) {
        self.handle_keyboard_shelf(ctx);

        if !self.shelf_loaded {
            self.shelf_loaded = true;
            self.refresh_entries();
            self.load_all_thumbnails(ctx);
        }

        egui::CentralPanel::default()
            .frame(egui::Frame {
                fill: BG,
                corner_radius: CornerRadius::same(14),
                inner_margin: egui::Margin::symmetric(12, 10),
                stroke: egui::Stroke::new(1.0, BORDER),
                shadow: egui::epaint::Shadow {
                    offset: [0, 6],
                    blur: 24,
                    spread: 0,
                    color: Color32::from_black_alpha(90),
                },
                ..Default::default()
            })
            .show(ctx, |ui| {
                self.render_header(ui);
                ui.add_space(8.0);

                let label_h = 15.0;
                let card_h = (ui.available_height() - label_h - 4.0).clamp(96.0, 150.0);
                let card_w = (card_h * 1.28).round();

                egui::ScrollArea::horizontal()
                    .auto_shrink([false, false])
                    .id_salt("shelf_scroll")
                    .show(ui, |ui| {
                        ui.set_min_height(card_h + label_h);
                        ui.spacing_mut().item_spacing.x = 8.0;

                        if self.entries.is_empty() {
                            ui.vertical_centered(|ui| {
                                ui.add_space((card_h + label_h) / 2.0 - 10.0);
                                ui.label(
                                    egui::RichText::new(if self.search_query.is_empty() {
                                        "Clipboard history is empty"
                                    } else {
                                        "No matches"
                                    })
                                    .font(FontId::proportional(13.0))
                                    .color(FG_DIM),
                                );
                            });
                            return;
                        }

                        // Group consecutive entries by day (Today / Yesterday /
                        // date) so each group gets its own inline heading.
                        ui.horizontal_top(|ui| {
                            let entries = self.entries.clone();
                            let mut i = 0;
                            while i < entries.len() {
                                let label = day_label(entries[i].timestamp);
                                let mut j = i;
                                while j < entries.len() && day_label(entries[j].timestamp) == label
                                {
                                    j += 1;
                                }
                                ui.vertical(|ui| {
                                    ui.add_space(1.0);
                                    // `extend()` keeps the heading on one line;
                                    // in a horizontal scroll the group's width
                                    // is otherwise ~0 and the text wraps to a
                                    // vertical letter-per-line stack.
                                    ui.add(
                                        egui::Label::new(
                                            egui::RichText::new(&label)
                                                .font(FontId::proportional(11.0))
                                                .color(FG_DIM),
                                        )
                                        .extend(),
                                    );
                                    ui.add_space(2.0);
                                    ui.horizontal_top(|ui| {
                                        for (k, entry) in entries[i..j].iter().enumerate() {
                                            self.render_card(ui, ctx, entry, i + k, card_w, card_h);
                                        }
                                    });
                                });
                                ui.add_space(4.0);
                                i = j;
                            }
                        });
                    });

                self.scroll_to_selected = false;
            });
    }

    fn render_header(&mut self, ui: &mut egui::Ui) {
        // Row 1: search box (left, fills) + icon buttons (right).
        ui.horizontal(|ui| {
            let btn = 30.0;
            let gap = 6.0;
            // Keep the button cluster clear of the panel's rounded corner.
            let right_inset = 8.0;
            let buttons_w = 3.0 * btn + 2.0 * gap;
            let search_w = (ui.available_width() - buttons_w - 8.0 - right_inset).max(140.0);
            self.search_box(ui, search_w);
            ui.add_space(8.0);

            let fav_active = self.active_filter.as_deref() == Some("favorites");
            if self.top_button(ui, HeaderIcon::Star, fav_active) {
                self.active_filter = if fav_active {
                    None
                } else {
                    Some("favorites".into())
                };
                self.refresh_entries();
            }
            ui.add_space(gap);
            if self.top_button(ui, HeaderIcon::Ocr, false) {
                self.trigger_ocr();
            }
            ui.add_space(gap);
            if self.top_button(ui, HeaderIcon::Close, false) {
                self.should_hide = true;
            }
        });

        ui.add_space(7.0);

        // Row 2: tabs with count badges.
        ui.horizontal(|ui| {
            let (all, text, image, _fav) = self.counts;
            let fav_active = self.active_filter.as_deref() == Some("favorites");
            let tabs: [(&str, Option<&str>, usize); 3] = [
                ("History", None, all),
                ("Text", Some("text"), text),
                ("Image", Some("image"), image),
            ];
            for (label, key, count) in tabs {
                let active = !fav_active && self.active_filter.as_deref() == key;
                if self.tab(ui, label, count, active) {
                    self.active_filter = key.map(|s| s.to_owned());
                    self.refresh_entries();
                }
                ui.add_space(6.0);
            }

            if let Some((msg, _)) = self.ocr_status.clone() {
                ui.add_space(2.0);
                ui.label(
                    egui::RichText::new(msg)
                        .font(FontId::proportional(11.0))
                        .color(GREEN),
                );
            }
        });
    }

    /// Rounded search field with a leading magnifier icon.
    fn search_box(&mut self, ui: &mut egui::Ui, width: f32) {
        egui::Frame::new()
            .fill(SEARCH_BG)
            .corner_radius(9)
            .inner_margin(egui::Margin::symmetric(9, 5))
            .show(ui, |ui| {
                ui.set_width((width - 18.0).max(80.0));
                ui.horizontal(|ui| {
                    let (ir, _) = ui.allocate_exact_size(Vec2::splat(13.0), egui::Sense::hover());
                    draw_search_icon(&ui.painter_at(ir), ir, FG_DIM);
                    ui.add_space(6.0);
                    let te = egui::TextEdit::singleline(&mut self.search_query)
                        .frame(false)
                        .hint_text("Search")
                        .font(FontId::proportional(13.0))
                        .text_color(FG)
                        .desired_width(f32::INFINITY);
                    if ui.add(te).changed() {
                        self.refresh_entries();
                    }
                });
            });
    }

    /// A rounded-square header icon button. Returns true when clicked.
    fn top_button(&self, ui: &mut egui::Ui, icon: HeaderIcon, active: bool) -> bool {
        let (rect, resp) = ui.allocate_exact_size(Vec2::splat(30.0), egui::Sense::click());
        let p = ui.painter_at(rect);
        let bg = if active {
            ACCENT
        } else if resp.hovered() {
            Color32::from_rgb(0x30, 0x34, 0x4c)
        } else {
            BTN_BG
        };
        p.rect_filled(rect, CornerRadius::same(9), bg);
        let fg = if active { TAB_ACTIVE_FG } else { FG_MUTED };
        match icon {
            HeaderIcon::Star => {
                p.text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    "★",
                    FontId::proportional(14.0),
                    if active { TAB_ACTIVE_FG } else { YELLOW },
                );
            }
            HeaderIcon::Ocr => draw_ocr_icon(&p, rect, fg),
            HeaderIcon::Close => {
                p.text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    "×",
                    FontId::proportional(16.0),
                    fg,
                );
            }
        }
        resp.on_hover_text(icon.tooltip()).clicked()
    }

    /// A tab pill with a trailing count badge; active tab is light-filled.
    fn tab(&self, ui: &mut egui::Ui, label: &str, count: usize, active: bool) -> bool {
        let font = FontId::proportional(12.5);
        let cfont = FontId::proportional(10.5);
        let lg = ui.fonts(|f| f.layout_no_wrap(label.to_owned(), font.clone(), Color32::WHITE));
        let cg = ui.fonts(|f| f.layout_no_wrap(count.to_string(), cfont.clone(), Color32::WHITE));
        let pad_x = 11.0;
        let gap = 6.0;
        let chip_w = cg.size().x + 10.0;
        let w = pad_x * 2.0 + lg.size().x + gap + chip_w;
        let (rect, resp) = ui.allocate_exact_size(Vec2::new(w, 28.0), egui::Sense::click());
        let p = ui.painter_at(rect);

        let bg = if active {
            TAB_ACTIVE_BG
        } else if resp.hovered() {
            Color32::from_rgb(0x24, 0x27, 0x3a)
        } else {
            ENTRY_BG
        };
        p.rect_filled(rect, CornerRadius::same(9), bg);

        let tcol = if active { TAB_ACTIVE_FG } else { FG_MUTED };
        p.text(
            egui::pos2(rect.left() + pad_x, rect.center().y),
            egui::Align2::LEFT_CENTER,
            label,
            font,
            tcol,
        );

        let chip_x = rect.left() + pad_x + lg.size().x + gap;
        let chip = egui::Rect::from_min_size(
            egui::pos2(chip_x, rect.center().y - 8.0),
            Vec2::new(chip_w, 16.0),
        );
        let chip_bg = if active {
            Color32::from_rgb(0xcf, 0xd6, 0xea)
        } else {
            Color32::from_rgb(0x2a, 0x2e, 0x42)
        };
        p.rect_filled(chip, CornerRadius::same(8), chip_bg);
        p.text(
            chip.center(),
            egui::Align2::CENTER_CENTER,
            count.to_string(),
            cfont,
            if active {
                Color32::from_rgb(0x4a, 0x50, 0x68)
            } else {
                FG_DIM
            },
        );
        resp.clicked()
    }

    /// Render one clipboard entry as a fixed-size card. Handles its own click
    /// (paste), hover action buttons (favorite/delete), right-click (context
    /// menu) and keyboard-driven scroll-into-view.
    fn render_card(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        entry: &ClipboardEntry,
        index: usize,
        w: f32,
        h: f32,
    ) {
        let is_selected = self.selected_index == Some(index);
        let is_image = entry.content_type == "image";
        let swatch = if is_image {
            None
        } else {
            parse_hex_color(&entry.content)
        };
        let radius = CornerRadius::same(10);

        let (rect, resp) = ui.allocate_exact_size(Vec2::new(w, h), egui::Sense::click());
        if !ui.is_rect_visible(rect) {
            return;
        }
        let is_hovered = resp.hovered();
        let painter = ui.painter_at(rect);
        let meta_top = rect.bottom() - 22.0;
        let meta_cy = meta_top + 11.0;

        // Card background (a color swatch fills the whole card).
        let card_bg = swatch.unwrap_or(if is_selected {
            ENTRY_BG_SELECTED
        } else {
            ENTRY_BG
        });
        painter.rect_filled(rect, radius, card_bg);
        let meta_fg = swatch.map(contrast_dim).unwrap_or(FG_DIM);

        // Content.
        if is_image {
            if !self.thumbnails.contains_key(&entry.id) {
                self.load_thumbnail(ctx, entry.id, entry);
            }
            let inner = egui::Rect::from_min_max(
                rect.min + Vec2::splat(6.0),
                egui::pos2(rect.right() - 6.0, meta_top - 2.0),
            );
            painter.rect_filled(inner, CornerRadius::same(6), IMG_BG);
            if let Some(tex) = self.thumbnails.get(&entry.id) {
                let tsz = tex.size_vec2();
                if tsz.x > 0.0 && tsz.y > 0.0 {
                    let scale = (inner.width() / tsz.x).min(inner.height() / tsz.y);
                    let draw = tsz * scale;
                    let img_rect = egui::Rect::from_center_size(inner.center(), draw);
                    painter.with_clip_rect(inner).image(
                        tex.id(),
                        img_rect,
                        egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                        Color32::WHITE,
                    );
                }
            } else {
                painter.text(
                    inner.center(),
                    egui::Align2::CENTER_CENTER,
                    "IMG",
                    FontId::monospace(13.0),
                    FG_DIM,
                );
            }
        } else if let Some(c) = swatch {
            // Color swatch: show the hex value in a contrasting colour.
            painter.text(
                egui::pos2(rect.left() + 12.0, rect.top() + 14.0),
                egui::Align2::LEFT_TOP,
                format!("#{:02X}{:02X}{:02X}", c.r(), c.g(), c.b()),
                FontId::monospace(13.0),
                contrast_text(c),
            );
        } else {
            let text_rect = egui::Rect::from_min_max(
                rect.min + Vec2::new(11.0, 9.0),
                egui::pos2(rect.right() - 9.0, meta_top - 2.0),
            );
            let content = entry.content.trim();
            let galley = ui.fonts(|f| {
                f.layout(
                    content.to_owned(),
                    FontId::monospace(12.0),
                    FG,
                    text_rect.width(),
                )
            });
            ui.painter_at(text_rect).galley(text_rect.min, galley, FG);
        }

        // Bottom meta row: app icon + relative time (left), size (right, images).
        let mut meta_x = rect.left() + 9.0;
        if let Some(src) = entry.source.as_deref().filter(|s| !s.is_empty()) {
            let icon =
                egui::Rect::from_min_size(egui::pos2(meta_x, meta_cy - 7.5), Vec2::splat(15.0));
            if let Some(tex) = self.app_icon(ctx, src) {
                painter.image(
                    tex.id(),
                    icon,
                    egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                    Color32::WHITE,
                );
            } else {
                painter.circle_filled(icon.center(), 7.5, OVERLAY_BTN_BG);
                let letter = icons::short_name(src).chars().next().unwrap_or('?');
                painter.text(
                    icon.center(),
                    egui::Align2::CENTER_CENTER,
                    letter.to_uppercase().to_string(),
                    FontId::proportional(9.0),
                    FG,
                );
            }
            meta_x += 15.0 + 5.0;
        }
        painter.text(
            egui::pos2(meta_x, meta_cy),
            egui::Align2::LEFT_CENTER,
            relative_time(entry.timestamp),
            FontId::monospace(9.5),
            meta_fg,
        );
        if is_image {
            let size = self.image_size(entry);
            if size > 0 {
                painter.text(
                    egui::pos2(rect.right() - 9.0, meta_cy),
                    egui::Align2::RIGHT_CENTER,
                    human_size(size),
                    FontId::monospace(9.5),
                    meta_fg,
                );
            }
        }

        // Action button rects: open (top-left, images only), favorite + delete
        // (top-right). Dispatched from the card's single click by pointer
        // position; no nested interactive widgets (which would steal hover).
        let (open_rect, fav_rect, del_rect) = card_button_rects(rect);
        let show_open = is_image;

        // Favorite badge (top-right) when favorited and not showing the actions.
        if entry.favorite && !is_hovered {
            painter.circle_filled(fav_rect.center(), 11.0, OVERLAY_BTN_BG);
            painter.text(
                fav_rect.center(),
                egui::Align2::CENTER_CENTER,
                "★",
                FontId::proportional(12.0),
                YELLOW,
            );
        }

        // Selection / hover border.
        let (bw, bc) = if is_selected {
            (2.0, ACCENT)
        } else if is_hovered {
            (1.5, BORDER_HOVER)
        } else {
            (1.0, BORDER)
        };
        painter.rect_stroke(
            rect,
            radius,
            egui::Stroke::new(bw, bc),
            egui::StrokeKind::Inside,
        );

        if is_hovered {
            let hover_pos = resp.hover_pos();
            if show_open {
                draw_circle_button(
                    &painter,
                    open_rect,
                    Glyph::Open,
                    FG,
                    hover_pos.is_some_and(|p| open_rect.contains(p)),
                );
            }
            let star = if entry.favorite { "★" } else { "☆" };
            let star_color = if entry.favorite { YELLOW } else { FG };
            draw_circle_button(
                &painter,
                fav_rect,
                Glyph::Text(star),
                star_color,
                hover_pos.is_some_and(|p| fav_rect.contains(p)),
            );
            draw_circle_button(
                &painter,
                del_rect,
                Glyph::Trash,
                RED,
                hover_pos.is_some_and(|p| del_rect.contains(p)),
            );

            self.hovered_entry_id = Some(entry.id);
            if is_image {
                self.last_image_hovered = Some(entry.id);
            }
        } else if self.hovered_entry_id == Some(entry.id) {
            self.hovered_entry_id = None;
        }

        // Transient "Copied" confirmation badge.
        if self.copied.map(|(id, _)| id) == Some(entry.id) {
            let bw = 78.0;
            let br = egui::Rect::from_center_size(
                egui::pos2(rect.center().x, rect.top() + 18.0),
                Vec2::new(bw, 22.0),
            );
            painter.rect_filled(
                br,
                CornerRadius::same(11),
                Color32::from_rgb(0x14, 0x2a, 0x1c),
            );
            painter.text(
                br.center(),
                egui::Align2::CENTER_CENTER,
                "✓ Copied",
                FontId::proportional(11.0),
                GREEN,
            );
        }

        if is_selected && self.scroll_to_selected {
            ui.scroll_to_rect(rect, Some(egui::Align::Center));
        }

        if resp.secondary_clicked() {
            self.context_menu_entry_id = Some(entry.id);
            self.context_menu_card_rect = Some(rect);
            self.context_menu_visible = true;
        }

        if resp.clicked() {
            // Route the click: action buttons take priority over paste.
            let p = resp.interact_pointer_pos();
            if show_open && p.is_some_and(|p| open_rect.contains(p)) {
                open_image(entry, &self.store);
            } else if p.is_some_and(|p| fav_rect.contains(p)) {
                self.store.lock().unwrap().toggle_favorite(entry.id).ok();
                self.refresh_entries();
            } else if p.is_some_and(|p| del_rect.contains(p)) {
                self.delete_confirm_entry_id = Some(entry.id);
                self.delete_confirm_visible = true;
            } else {
                self.selected_index = Some(index);
                paste_entry(entry, &self.store);
                self.copied = Some((entry.id, Instant::now()));
                self.pending_hide = Some(Instant::now());
            }
        }
    }

    /// Cached app-logo texture for a window class, loading it on first use.
    fn app_icon(&mut self, ctx: &egui::Context, class: &str) -> Option<TextureHandle> {
        if let Some(cached) = self.app_icons.get(class) {
            return cached.clone();
        }
        let tex = icons::resolve(class).and_then(|path| load_icon_texture(ctx, &path, class));
        self.app_icons.insert(class.to_owned(), tex.clone());
        tex
    }

    /// On-disk byte size of an image entry (cached).
    fn image_size(&mut self, entry: &ClipboardEntry) -> u64 {
        if let Some(&s) = self.image_sizes.get(&entry.id) {
            return s;
        }
        let s = entry
            .content_path
            .as_deref()
            .and_then(|p| std::fs::metadata(p).ok())
            .map(|m| m.len())
            .unwrap_or(0);
        self.image_sizes.insert(entry.id, s);
        s
    }

    /// Start an OCR job on the last-hovered (or selected) image, off-thread.
    fn trigger_ocr(&mut self) {
        if !self.ocr_available {
            self.ocr_status = Some(("tesseract not installed".into(), Instant::now()));
            return;
        }
        if self.ocr_pending.is_some() {
            return;
        }
        let target = self.last_image_hovered.or_else(|| {
            self.selected_index
                .and_then(|i| self.entries.get(i))
                .map(|e| e.id)
        });
        let entry = target.and_then(|id| self.entries.iter().find(|e| e.id == id).cloned());
        let Some(entry) = entry.filter(|e| e.content_type == "image") else {
            self.ocr_status = Some(("Hover an image to scan".into(), Instant::now()));
            return;
        };
        let Ok(Some(data)) = self.store.lock().unwrap().get_image_data(entry.id) else {
            return;
        };
        let slot = Arc::new(Mutex::new(None));
        self.ocr_pending = Some(slot.clone());
        self.ocr_status = Some(("Scanning…".into(), Instant::now()));
        std::thread::spawn(move || {
            let text = ocr::recognize(entry.id, &data).unwrap_or_default();
            *slot.lock().unwrap() = Some(text);
        });
    }

    /// Apply an OCR result once the worker thread has produced it.
    fn poll_ocr(&mut self) {
        let Some(slot) = self.ocr_pending.clone() else {
            return;
        };
        let Some(text) = slot.lock().unwrap().take() else {
            return;
        };
        self.ocr_pending = None;
        if text.is_empty() {
            self.ocr_status = Some(("No text found".into(), Instant::now()));
        } else {
            set_clipboard(&text);
            self.store
                .lock()
                .unwrap()
                .insert(&text, "text", Some("ocr"))
                .ok();
            self.refresh_entries();
            self.ocr_status = Some(("Text copied".into(), Instant::now()));
        }
    }

    fn load_all_thumbnails(&mut self, ctx: &egui::Context) {
        let entries: Vec<(i64, ClipboardEntry)> = self
            .entries
            .iter()
            .filter(|e| e.content_type == "image")
            .map(|e| (e.id, e.clone()))
            .collect();
        let count = entries.len();
        for (id, entry) in entries {
            self.load_thumbnail(ctx, id, &entry);
        }
        tracing::debug!("loaded {} thumbnails for shelf", count);
    }
}

/// Header icon-button kinds.
#[derive(Clone, Copy)]
enum HeaderIcon {
    Star,
    Ocr,
    Close,
}

impl HeaderIcon {
    fn tooltip(self) -> &'static str {
        match self {
            HeaderIcon::Star => "Favorites",
            HeaderIcon::Ocr => "Scan text from image (OCR)",
            HeaderIcon::Close => "Close",
        }
    }
}

/// Card hover-button glyphs (painted, so they never render as tofu).
enum Glyph<'a> {
    Open,
    Trash,
    Text(&'a str),
}

/// `(open, favorite, delete)` action-button rects for a card. `open` is
/// top-left (images), favorite/delete are top-right. Both drawing and
/// hit-testing use this single source of truth.
fn card_button_rects(card: egui::Rect) -> (egui::Rect, egui::Rect, egui::Rect) {
    const BS: f32 = 22.0;
    let open = egui::Rect::from_min_size(
        egui::pos2(card.left() + 6.0, card.top() + 6.0),
        Vec2::splat(BS),
    );
    let del = egui::Rect::from_min_size(
        egui::pos2(card.right() - 6.0 - BS, card.top() + 6.0),
        Vec2::splat(BS),
    );
    let fav = egui::Rect::from_min_size(
        egui::pos2(del.left() - 5.0 - BS, card.top() + 6.0),
        Vec2::splat(BS),
    );
    (open, fav, del)
}

/// Draw a circular hover-action button with a painted or text glyph.
fn draw_circle_button(
    painter: &egui::Painter,
    rect: egui::Rect,
    glyph: Glyph,
    color: Color32,
    hovered: bool,
) {
    let bg = if hovered {
        Color32::from_rgb(0x2a, 0x2e, 0x45)
    } else {
        OVERLAY_BTN_BG
    };
    painter.circle_filled(rect.center(), rect.width() / 2.0, bg);
    match glyph {
        Glyph::Open => draw_open_icon(painter, rect, color),
        Glyph::Trash => draw_trash_icon(painter, rect, color),
        Glyph::Text(s) => {
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                s,
                FontId::proportional(12.0),
                color,
            );
        }
    }
}

/// "Open external" arrow (↗ out of a box), painted into `rect`.
fn draw_open_icon(painter: &egui::Painter, rect: egui::Rect, color: Color32) {
    let s = egui::Stroke::new(1.4, color);
    let c = rect.center();
    let r = rect.width() * 0.24;
    // Box (bottom-left).
    let bl = egui::Rect::from_min_max(
        egui::pos2(c.x - r, c.y - r + 2.0),
        egui::pos2(c.x + r - 2.0, c.y + r),
    );
    painter.rect_stroke(bl, CornerRadius::same(2), s, egui::StrokeKind::Middle);
    // Arrow out of the top-right.
    let a0 = egui::pos2(c.x, c.y - 1.0);
    let a1 = egui::pos2(c.x + r + 1.5, c.y - r - 1.5);
    painter.line_segment([a0, a1], s);
    painter.line_segment([a1, egui::pos2(a1.x - 4.0, a1.y)], s);
    painter.line_segment([a1, egui::pos2(a1.x, a1.y + 4.0)], s);
}

/// Trash-can icon painted into `rect`.
fn draw_trash_icon(painter: &egui::Painter, rect: egui::Rect, color: Color32) {
    let s = egui::Stroke::new(1.3, color);
    let c = rect.center();
    let w = rect.width() * 0.30;
    let top = c.y - w * 0.7;
    // Lid.
    painter.line_segment([egui::pos2(c.x - w, top), egui::pos2(c.x + w, top)], s);
    // Handle.
    painter.line_segment(
        [
            egui::pos2(c.x - w * 0.4, top - 2.0),
            egui::pos2(c.x + w * 0.4, top - 2.0),
        ],
        s,
    );
    // Body.
    let body = egui::Rect::from_min_max(
        egui::pos2(c.x - w * 0.8, top + 1.5),
        egui::pos2(c.x + w * 0.8, c.y + w * 0.9),
    );
    painter.rect_stroke(body, CornerRadius::same(1), s, egui::StrokeKind::Middle);
}

/// Viewfinder icon (four corner brackets) for the OCR button.
fn draw_ocr_icon(painter: &egui::Painter, rect: egui::Rect, color: Color32) {
    let s = egui::Stroke::new(1.5, color);
    let b = rect.shrink(rect.width() * 0.30);
    let l = 4.0;
    let corners = [
        (b.left_top(), Vec2::new(l, 0.0), Vec2::new(0.0, l)),
        (b.right_top(), Vec2::new(-l, 0.0), Vec2::new(0.0, l)),
        (b.left_bottom(), Vec2::new(l, 0.0), Vec2::new(0.0, -l)),
        (b.right_bottom(), Vec2::new(-l, 0.0), Vec2::new(0.0, -l)),
    ];
    for (p, a, c) in corners {
        painter.line_segment([p, p + a], s);
        painter.line_segment([p, p + c], s);
    }
    // Scan line through the middle.
    painter.line_segment(
        [
            egui::pos2(b.left() + 1.0, b.center().y),
            egui::pos2(b.right() - 1.0, b.center().y),
        ],
        egui::Stroke::new(1.5, color.linear_multiply(0.8)),
    );
}

/// Magnifier icon for the search field.
fn draw_search_icon(painter: &egui::Painter, rect: egui::Rect, color: Color32) {
    let s = egui::Stroke::new(1.4, color);
    let c = egui::pos2(
        rect.left() + rect.width() * 0.42,
        rect.top() + rect.height() * 0.42,
    );
    let r = rect.width() * 0.30;
    painter.circle_stroke(c, r, s);
    let d = r * 0.72;
    painter.line_segment(
        [
            egui::pos2(c.x + d, c.y + d),
            egui::pos2(rect.right() - 1.0, rect.bottom() - 1.0),
        ],
        s,
    );
}

/// Parse a clipboard string as a hex colour (`#RGB`, `#RRGGBB`, with or
/// without the leading `#`), for rendering color-swatch cards.
fn parse_hex_color(s: &str) -> Option<Color32> {
    let h = s.trim().trim_start_matches('#');
    if !h.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let byte = |i: usize| u8::from_str_radix(&h[i..i + 2], 16).ok();
    match h.len() {
        6 => Some(Color32::from_rgb(byte(0)?, byte(2)?, byte(4)?)),
        3 => {
            let d = |i: usize| u8::from_str_radix(&h[i..i + 1], 16).ok().map(|v| v * 17);
            Some(Color32::from_rgb(d(0)?, d(1)?, d(2)?))
        }
        _ => None,
    }
}

/// Black or white, whichever reads better on `bg`.
fn contrast_text(bg: Color32) -> Color32 {
    let lum = 0.299 * bg.r() as f32 + 0.587 * bg.g() as f32 + 0.114 * bg.b() as f32;
    if lum > 140.0 {
        Color32::from_rgb(0x10, 0x12, 0x18)
    } else {
        Color32::WHITE
    }
}

/// Dimmed contrast colour for meta text over a swatch.
fn contrast_dim(bg: Color32) -> Color32 {
    contrast_text(bg).gamma_multiply(0.72)
}

/// Day bucket label for grouping: `Today`, `Yesterday`, or `Mon D`.
fn day_label(ts: chrono::DateTime<chrono::Utc>) -> String {
    use chrono::{Datelike, Local};
    let today = Local::now().date_naive();
    let d = ts.with_timezone(&Local).date_naive();
    let delta = (today - d).num_days();
    if delta <= 0 {
        "Today".to_owned()
    } else if delta == 1 {
        "Yesterday".to_owned()
    } else {
        format!("{} {}", month_abbr(d.month()), d.day())
    }
}

fn month_abbr(m: u32) -> &'static str {
    const M: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    M[(m.clamp(1, 12) - 1) as usize]
}

/// Human-readable byte size (e.g. `364 KB`, `1.2 MB`).
fn human_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{} KB", bytes / KB)
    } else {
        format!("{} B", bytes)
    }
}

/// Load and cache an app-icon PNG as a small egui texture.
fn load_icon_texture(
    ctx: &egui::Context,
    path: &std::path::Path,
    class: &str,
) -> Option<TextureHandle> {
    let bytes = std::fs::read(path).ok()?;
    let img = image::load_from_memory(&bytes).ok()?;
    let img = img.thumbnail(32, 32).to_rgba8();
    let (w, h) = img.dimensions();
    if w == 0 || h == 0 {
        return None;
    }
    let color = egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], &img.into_raw());
    Some(ctx.load_texture(
        format!("appicon_{class}"),
        color,
        egui::TextureOptions::LINEAR,
    ))
}

/// Open an image entry in the system default viewer.
fn open_image(entry: &ClipboardEntry, store: &Arc<Mutex<Store>>) {
    let path = entry
        .content_path
        .clone()
        .filter(|p| std::path::Path::new(p).exists())
        .or_else(|| {
            let data = store
                .lock()
                .unwrap()
                .get_image_data(entry.id)
                .ok()
                .flatten()?;
            let tmp = std::env::temp_dir().join(format!("clipvault-open-{}.png", entry.id));
            std::fs::write(&tmp, data).ok()?;
            Some(tmp.to_string_lossy().to_string())
        });
    if let Some(path) = path {
        let _ = std::process::Command::new("xdg-open").arg(path).spawn();
    }
}

/// Compact "time ago" label (e.g. `now`, `5m`, `3h`, `2d`).
fn relative_time(ts: chrono::DateTime<chrono::Utc>) -> String {
    let secs = (chrono::Utc::now() - ts).num_seconds().max(0);
    if secs < 60 {
        "now".to_owned()
    } else if secs < 3600 {
        format!("{} min ago", secs / 60)
    } else if secs < 86_400 {
        format!("{} hr ago", secs / 3600)
    } else {
        format!("{}d ago", secs / 86_400)
    }
}

fn paste_entry(entry: &ClipboardEntry, store: &Arc<Mutex<Store>>) {
    if entry.content_type == "image" {
        if let Ok(Some(data)) = store.lock().unwrap().get_image_data(entry.id) {
            let mime = entry
                .content
                .strip_prefix("image/")
                .unwrap_or("png")
                .to_string();
            set_clipboard_image(&data, &format!("image/{}", mime));
        }
    } else {
        set_clipboard(&entry.content);
    }
}

fn set_clipboard(content: &str) {
    let mut child = match std::process::Command::new("wl-copy")
        .stdin(std::process::Stdio::piped())
        .spawn()
    {
        Ok(c) => c,
        Err(_) => return,
    };
    if let Some(mut stdin) = child.stdin.take() {
        use std::io::Write;
        let _ = stdin.write_all(content.as_bytes());
    }
    let _ = child.wait();
}

fn set_clipboard_image(data: &[u8], mime: &str) {
    let mut child = match std::process::Command::new("wl-copy")
        .args(["--type", mime])
        .stdin(std::process::Stdio::piped())
        .spawn()
    {
        Ok(c) => c,
        Err(_) => return,
    };
    if let Some(mut stdin) = child.stdin.take() {
        use std::io::Write;
        let _ = stdin.write_all(data);
    }
    let _ = child.wait();
}

fn fetch_entries(
    store: &Arc<Mutex<Store>>,
    filter: Option<&str>,
    category: Option<&str>,
    limit: usize,
    offset: usize,
) -> Vec<ClipboardEntry> {
    let store = store.lock().unwrap();
    match (filter, category) {
        (Some(ft), Some(cat)) => store.list_by_type_and_category(ft, cat, limit, offset),
        (Some(ft), None) => store.list_by_type(ft, limit, offset),
        (None, Some(cat)) => store.list_by_category(cat, limit, offset),
        (None, None) => store.list(limit, offset),
    }
    .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn card() -> egui::Rect {
        egui::Rect::from_min_size(egui::pos2(100.0, 200.0), Vec2::new(115.0, 115.0))
    }

    #[test]
    fn should_place_action_buttons_inside_card() {
        let (open, fav, del) = card_button_rects(card());
        assert!(
            card().contains(open.center())
                && card().contains(fav.center())
                && card().contains(del.center())
        );
    }

    #[test]
    fn should_not_overlap_favorite_and_delete_buttons() {
        let (_open, fav, del) = card_button_rects(card());
        assert!(!fav.intersects(del));
    }

    #[test]
    fn should_put_open_button_on_the_left_and_delete_on_the_right() {
        let (open, _fav, del) = card_button_rects(card());
        assert!(open.center().x < del.center().x);
    }

    #[test]
    fn should_route_card_body_click_to_no_button() {
        let (open, fav, del) = card_button_rects(card());
        let body = card().center();
        assert!(!open.contains(body) && !fav.contains(body) && !del.contains(body));
    }

    #[test]
    fn should_parse_six_digit_hex_color() {
        assert_eq!(
            parse_hex_color("#FF8F4A"),
            Some(Color32::from_rgb(255, 143, 74))
        );
    }

    #[test]
    fn should_parse_hex_without_hash() {
        assert_eq!(
            parse_hex_color("2110A1"),
            Some(Color32::from_rgb(33, 16, 161))
        );
    }

    #[test]
    fn should_reject_non_hex_as_color() {
        assert!(parse_hex_color("hello world").is_none());
    }

    #[test]
    fn should_format_human_sizes() {
        assert_eq!(human_size(364 * 1024), "364 KB");
    }
}
