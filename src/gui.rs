use crate::config::Config;
use crate::store::{ClipboardEntry, Store};
use eframe::egui;
use egui::{
    Color32, CornerRadius, FontFamily, FontId, Key, TextureHandle, Vec2, Widget,
};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

const BG: Color32 = Color32::from_rgb(0x1a, 0x1b, 0x26);
const FG: Color32 = Color32::from_rgb(0xa9, 0xb1, 0xd6);
const FG_DIM: Color32 = Color32::from_rgb(0x56, 0x5f, 0x89);
const RED: Color32 = Color32::from_rgb(0xf7, 0x76, 0x8e);
const GREEN: Color32 = Color32::from_rgb(0x9e, 0xce, 0x6a);
const ACCENT: Color32 = Color32::from_rgb(0x7a, 0xa2, 0xf7);
const ENTRY_BG: Color32 = Color32::from_rgb(0x1e, 0x20, 0x30);
const ENTRY_BG_SELECTED: Color32 = Color32::from_rgb(0x24, 0x27, 0x3a);

pub struct ClipboardApp {
    store: Arc<Mutex<Store>>,
    entries: Vec<ClipboardEntry>,
    search_query: String,
    selected_index: Option<usize>,
    active_filter: Option<String>,
    active_category: Option<String>,
    categories: Vec<(i64, String, Option<String>)>,
    thumbnails: HashMap<i64, TextureHandle>,
    thumb_size: f32,
    shelf_loaded: bool,
    context_menu_entry_id: Option<i64>,
    context_menu_visible: bool,
    context_menu_card_rect: Option<egui::Rect>,
    delete_confirm_entry_id: Option<i64>,
    delete_confirm_visible: bool,
    hovered_entry_id: Option<i64>,
    pub should_hide: bool,
}

impl ClipboardApp {
    pub fn new(store: Arc<Mutex<Store>>, config: &Config) -> Self {
        let categories = store.lock().unwrap().list_categories().unwrap_or_default();
        let entries = fetch_entries(&store, None, None, 50, 0);
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
            thumb_size: config.shelf_thumb_size,
            shelf_loaded: false,
            context_menu_entry_id: None,
            context_menu_visible: false,
            context_menu_card_rect: None,
            delete_confirm_entry_id: None,
            delete_confirm_visible: false,
            hovered_entry_id: None,
            should_hide: false,
        }
    }

    fn refresh_entries(&mut self) {
        let limit = 50;
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
            let thumb = img.thumbnail(self.thumb_size as u32, self.thumb_size as u32);
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
        if !ctx.wants_keyboard_input() {
            if ctx.input(|i| i.key_pressed(Key::ArrowLeft)) {
                let new_idx = self.selected_index.map_or(0, |i| i.saturating_sub(1));
                self.selected_index = Some(new_idx);
                return true;
            }
            if ctx.input(|i| i.key_pressed(Key::ArrowRight)) {
                let max = self.entries.len().saturating_sub(1);
                let new_idx = self
                    .selected_index
                    .map_or(0, |i| i.saturating_add(1).min(max));
                self.selected_index = Some(new_idx);
                return true;
            }
            if ctx.input(|i| i.key_pressed(Key::Enter))
                && let Some(idx) = self.selected_index
                && let Some(entry) = self.entries.get(idx)
            {
                paste_entry(entry, &self.store);
                return true;
            }
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
                                    egui::RichText::new("✕")
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

        ctx.request_repaint_after(std::time::Duration::from_millis(500));
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
                corner_radius: CornerRadius::same(12),
                shadow: egui::epaint::Shadow {
                    offset: [0, 4],
                    blur: 20,
                    spread: 0,
                    color: Color32::from_black_alpha(60),
                },
                ..Default::default()
            })
            .show(ctx, |ui| {
                ui.set_min_size(Vec2::new(780.0, 0.0));

                ui.horizontal(|ui| {
                    ui.add_space(6.0);
                    let filter_all = self.active_filter.is_none();
                    let filter_text = self.active_filter.as_deref() == Some("text");
                    let filter_image = self.active_filter.as_deref() == Some("image");

                    let pill_bg = Color32::from_rgb(0x24, 0x27, 0x3a);
                    let pill_selected = ACCENT;

                    if self
                        .filter_pill(ui, "All", filter_all, pill_bg, pill_selected)
                        .clicked()
                    {
                        self.active_filter = None;
                        self.refresh_entries();
                    }
                    ui.add_space(2.0);
                    if self
                        .filter_pill(ui, "Text", filter_text, pill_bg, pill_selected)
                        .clicked()
                    {
                        self.active_filter = Some("text".into());
                        self.refresh_entries();
                    }
                    ui.add_space(2.0);
                    if self
                        .filter_pill(ui, "Image", filter_image, pill_bg, pill_selected)
                        .clicked()
                    {
                        self.active_filter = Some("image".into());
                        self.refresh_entries();
                    }
                    ui.add_space(2.0);
                    let filter_fav = self.active_filter.as_deref() == Some("favorites");
                    if self
                        .filter_pill(ui, "Fav", filter_fav, pill_bg, pill_selected)
                        .clicked()
                    {
                        self.active_filter = Some("favorites".into());
                        self.refresh_entries();
                    }
                    ui.add_space(6.0);

                    let search = egui::TextEdit::singleline(&mut self.search_query)
                        .hint_text("Search...")
                        .font(FontId {
                            size: 11.0,
                            family: FontFamily::Monospace,
                        })
                        .text_color(FG)
                        .background_color(Color32::from_rgb(0x14, 0x15, 0x1e))
                        .desired_width(120.0);
                    let resp = ui.add(search);
                    if resp.changed() {
                        self.refresh_entries();
                    }

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let close_btn = egui::Button::new(
                            egui::RichText::new("✕")
                                .font(FontId {
                                    size: 12.0,
                                    family: FontFamily::Proportional,
                                })
                                .color(FG_DIM),
                        )
                        .fill(Color32::TRANSPARENT)
                        .min_size(Vec2::new(20.0, 20.0));
                        if ui.add(close_btn).clicked() {
                            self.should_hide = true;
                        }
                        ui.add_space(4.0);
                    });
                });

                ui.add_space(4.0);

                let card_size = self.thumb_size + 16.0;
                let available = ui.available_width() - 12.0;
                let _max_visible = (available / (card_size + 6.0)).floor() as usize;

                egui::ScrollArea::horizontal()
                    .auto_shrink([false, false])
                    .id_salt("shelf_scroll")
                    .show(ui, |ui| {
                        ui.set_min_height(card_size + 4.0);

                        if self.entries.is_empty() {
                            ui.add_space(20.0);
                            ui.label(
                                egui::RichText::new("Empty")
                                    .font(FontId {
                                        size: 11.0,
                                        family: FontFamily::Monospace,
                                    })
                                    .color(FG_DIM),
                            );
                            return;
                        }

                        ui.horizontal(|ui| {
                            ui.add_space(4.0);
                            let entries_to_render = self.entries.clone();
                            for (i, entry) in entries_to_render.iter().enumerate() {
                                let is_selected = self.selected_index == Some(i);
                                let is_image = entry.content_type == "image";
                                let is_hovered = self.hovered_entry_id == Some(entry.id);

                                let border_color = if is_selected {
                                    ACCENT
                                } else if is_hovered {
                                    Color32::from_rgb(0x39, 0x40, 0x60)
                                } else {
                                    Color32::from_rgb(0x31, 0x34, 0x4a)
                                };

                                let card_frame = egui::Frame {
                                    fill: if is_selected {
                                        ENTRY_BG_SELECTED
                                    } else {
                                        ENTRY_BG
                                    },
                                    corner_radius: CornerRadius::same(6),
                                    stroke: egui::Stroke::new(1.5, border_color),
                                    ..Default::default()
                                };

                                let resp = card_frame
                                    .show(ui, |ui| {
                                        ui.set_min_size(Vec2::new(card_size, card_size));
                                        ui.set_max_size(Vec2::new(
                                            card_size + 8.0,
                                            card_size + 8.0,
                                        ));

                                        if entry.favorite {
                                            ui.with_layout(
                                                egui::Layout::right_to_left(egui::Align::TOP),
                                                |ui| {
                                                    ui.add_space(4.0);
                                                    ui.label(
                                                        egui::RichText::new("★")
                                                            .font(FontId {
                                                                size: 10.0,
                                                                family: FontFamily::Proportional,
                                                            })
                                                            .color(GREEN),
                                                    );
                                                },
                                            );
                                        }

                                        if is_image {
                                            if let Some(tex) = self.thumbnails.get(&entry.id) {
                                                let img_size = self.thumb_size;
                                                ui.add_space((card_size - img_size) / 2.0);
                                                ui.horizontal(|ui| {
                                                    ui.add_space((card_size - img_size) / 2.0);
                                                    egui::Image::new(tex)
                                                        .max_width(img_size)
                                                        .max_height(img_size)
                                                        .corner_radius(CornerRadius::same(4))
                                                        .ui(ui);
                                                });
                                            } else {
                                                ui.vertical_centered(|ui| {
                                                    ui.add_space(card_size * 0.25);
                                                    ui.label(egui::RichText::new("🖼").font(
                                                        FontId {
                                                            size: card_size * 0.35,
                                                            family: FontFamily::Proportional,
                                                        },
                                                    ));
                                                });
                                            }
                                        } else {
                                            let preview =
                                                entry.content.lines().next().unwrap_or("");
                                            let short = if preview.len() > 18 {
                                                format!("{}…", &preview[..18])
                                            } else {
                                                preview.to_string()
                                            };
                                            ui.vertical_centered(|ui| {
                                                ui.add_space(8.0);
                                                ui.label(
                                                    egui::RichText::new(&short)
                                                        .font(FontId {
                                                            size: 9.0,
                                                            family: FontFamily::Monospace,
                                                        })
                                                        .color(FG),
                                                );
                                            });
                                        }

                                        if let Some(ref cat_name) = entry.category {
                                            ui.add_space(2.0);
                                            ui.horizontal(|ui| {
                                                ui.add_space(4.0);
                                                let cat_color = ACCENT;
                                                egui::Frame::default()
                                                    .fill(cat_color.linear_multiply(0.3))
                                                    .corner_radius(CornerRadius::same(3))
                                                    .inner_margin(egui::Margin::symmetric(4, 1))
                                                    .show(ui, |ui| {
                                                        ui.label(
                                                            egui::RichText::new(cat_name)
                                                                .font(FontId {
                                                                    size: 7.0,
                                                                    family: FontFamily::Monospace,
                                                                })
                                                                .color(FG),
                                                        );
                                                    });
                                            });
                                        }

                                        if is_hovered {
                                            ui.add_space(2.0);
                                            ui.horizontal(|ui| {
                                                ui.add_space(4.0);
                                                let star =
                                                    if entry.favorite { "★" } else { "☆" };
                                                let sc =
                                                    if entry.favorite { GREEN } else { FG_DIM };
                                                if ui
                                                    .add(
                                                        egui::Button::new(
                                                            egui::RichText::new(star)
                                                                .font(FontId {
                                                                    size: 10.0,
                                                                    family:
                                                                        FontFamily::Proportional,
                                                                })
                                                                .color(sc),
                                                        )
                                                        .fill(Color32::TRANSPARENT)
                                                        .min_size(Vec2::new(16.0, 16.0)),
                                                    )
                                                    .clicked()
                                                {
                                                    self.store
                                                        .lock()
                                                        .unwrap()
                                                        .toggle_favorite(entry.id)
                                                        .ok();
                                                    self.refresh_entries();
                                                }
                                                if ui
                                                    .add(
                                                        egui::Button::new(
                                                            egui::RichText::new("🗑")
                                                                .font(FontId {
                                                                    size: 10.0,
                                                                    family:
                                                                        FontFamily::Proportional,
                                                                })
                                                                .color(RED),
                                                        )
                                                        .fill(Color32::TRANSPARENT)
                                                        .min_size(Vec2::new(16.0, 16.0)),
                                                    )
                                                    .clicked()
                                                {
                                                    self.delete_confirm_entry_id = Some(entry.id);
                                                    self.delete_confirm_visible = true;
                                                }
                                            });
                                        }
                                    })
                                    .response;

                                if resp.hovered() {
                                    self.hovered_entry_id = Some(entry.id);
                                } else if self.hovered_entry_id == Some(entry.id) {
                                    self.hovered_entry_id = None;
                                }

                                if resp.secondary_clicked() {
                                    self.context_menu_entry_id = Some(entry.id);
                                    self.context_menu_card_rect = Some(resp.rect);
                                    self.context_menu_visible = true;
                                }

                                if resp.clicked() {
                                    paste_entry(entry, &self.store);
                                }

                                ui.add_space(4.0);
                            }
                        });
                    });

                ui.add_space(4.0);
            });
    }

    fn filter_pill(
        &self,
        ui: &mut egui::Ui,
        label: &str,
        active: bool,
        bg: Color32,
        selected_color: Color32,
    ) -> egui::Response {
        let fill = if active { selected_color } else { bg };
        let text_color = if active { Color32::WHITE } else { FG_DIM };
        ui.add(
            egui::Button::new(
                egui::RichText::new(label)
                    .font(FontId {
                        size: 10.0,
                        family: FontFamily::Monospace,
                    })
                    .color(text_color),
            )
            .fill(fill)
            .corner_radius(10)
            .min_size(Vec2::new(4.0, 20.0)),
        )
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
