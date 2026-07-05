use crate::store::{ClipboardEntry, Store};
use eframe::egui;
use egui::{Color32, CornerRadius, FontFamily, FontId, Vec2, ViewportCommand};
use std::sync::{Arc, Mutex};

const BG: Color32 = Color32::from_rgb(0x1a, 0x1b, 0x26);
const FG: Color32 = Color32::from_rgb(0xa9, 0xb1, 0xd6);
const FG_DIM: Color32 = Color32::from_rgb(0x56, 0x5f, 0x89);
const RED: Color32 = Color32::from_rgb(0xf7, 0x76, 0x8e);
const GREEN: Color32 = Color32::from_rgb(0x9e, 0xce, 0x6a);

pub struct ClipboardApp {
    store: Arc<Mutex<Store>>,
    entries: Vec<ClipboardEntry>,
    search_query: String,
    hide_on_escape: bool,
}

impl ClipboardApp {
    pub fn new(store: Arc<Mutex<Store>>) -> Self {
        let entries = store.lock().unwrap().list(500, 0).unwrap_or_default();
        Self {
            store,
            entries,
            search_query: String::new(),
            hide_on_escape: true,
        }
    }

    fn refresh_entries(&mut self) {
        let store = self.store.lock().unwrap();
        if self.search_query.is_empty() {
            self.entries = store.list(500, 0).unwrap_or_default();
        } else {
            self.entries = store.search(&self.search_query, 500).unwrap_or_default();
        }
    }
}

impl eframe::App for ClipboardApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let mut should_close = false;

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

        egui::CentralPanel::default()
            .frame(egui::Frame {
                fill: BG,
                corner_radius: CornerRadius::same(12),
                shadow: egui::epaint::Shadow {
                    offset: [0, 8],
                    blur: 32,
                    spread: 0,
                    color: Color32::from_black_alpha(80),
                },
                ..Default::default()
            })
            .show(ctx, |ui| {
                ui.set_min_size(Vec2::new(560.0, 0.0));

                ui.horizontal(|ui| {
                    ui.add_space(8.0);
                    ui.label(egui::RichText::new("ClipVault").font(FontId {
                        size: 14.0,
                        family: FontFamily::Proportional,
                    }));

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let close_btn = egui::Button::new(
                            egui::RichText::new("✕")
                                .font(FontId {
                                    size: 13.0,
                                    family: FontFamily::Proportional,
                                })
                                .color(FG_DIM),
                        )
                        .fill(Color32::TRANSPARENT)
                        .min_size(Vec2::new(24.0, 24.0));
                        if ui.add(close_btn).clicked() {
                            should_close = true;
                        }
                        ui.add_space(4.0);
                    });
                });

                ui.add_space(4.0);

                ui.horizontal(|ui| {
                    ui.add_space(8.0);
                    let search = egui::TextEdit::singleline(&mut self.search_query)
                        .hint_text("Search clipboard history...")
                        .font(FontId {
                            size: 13.0,
                            family: FontFamily::Proportional,
                        })
                        .text_color(FG)
                        .background_color(Color32::from_rgb(0x14, 0x15, 0x1e))
                        .desired_width(f32::INFINITY);
                    let resp = ui.add(search);
                    if resp.changed() || !self.search_query.is_empty() {
                        self.refresh_entries();
                    }
                    ui.add_space(8.0);
                });

                ui.add_space(8.0);

                let count_text = format!(
                    "{} items{}",
                    self.entries.len(),
                    if self.search_query.is_empty() {
                        String::new()
                    } else {
                        " (filtered)".into()
                    }
                );
                ui.horizontal(|ui| {
                    ui.add_space(8.0);
                    ui.label(
                        egui::RichText::new(count_text)
                            .font(FontId {
                                size: 11.0,
                                family: FontFamily::Monospace,
                            })
                            .color(FG_DIM),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let clear_text = egui::RichText::new("clear history")
                            .font(FontId {
                                size: 11.0,
                                family: FontFamily::Monospace,
                            })
                            .color(RED);
                        if ui
                            .add(egui::Button::new(clear_text).fill(Color32::TRANSPARENT))
                            .clicked()
                        {
                            self.store.lock().unwrap().clear().ok();
                            self.refresh_entries();
                        }
                        ui.add_space(8.0);
                    });
                });

                ui.separator();

                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        if self.entries.is_empty() {
                            ui.add_space(40.0);
                            ui.vertical_centered(|ui| {
                                ui.label(
                                    egui::RichText::new("No clipboard history yet")
                                        .font(FontId {
                                            size: 13.0,
                                            family: FontFamily::Proportional,
                                        })
                                        .color(FG_DIM),
                                );
                            });
                            return;
                        }

                        let entries_to_render = self.entries.clone();
                        for entry in &entries_to_render {
                            let preview = entry.content.lines().next().unwrap_or("");
                            let max_preview = 120;
                            let preview_text = if preview.len() > max_preview {
                                format!("{}...", &preview[..max_preview])
                            } else {
                                preview.to_string()
                            };

                            let time_str = entry.timestamp.format("%H:%M").to_string();
                            let icon = if entry.favorite { "★" } else { "📋" };

                            let frame = egui::Frame {
                                fill: Color32::from_rgb(0x1e, 0x20, 0x30),
                                corner_radius: CornerRadius::same(6),
                                stroke: egui::Stroke::new(1.0, Color32::from_rgb(0x31, 0x34, 0x4a)),
                                ..Default::default()
                            };

                            let resp = frame
                                .show(ui, |ui| {
                                    ui.set_min_height(36.0);
                                    ui.horizontal(|ui| {
                                        ui.add_space(8.0);
                                        ui.label(egui::RichText::new(icon).font(FontId {
                                            size: 12.0,
                                            family: FontFamily::Proportional,
                                        }));
                                        ui.add_space(8.0);
                                        ui.label(
                                            egui::RichText::new(time_str)
                                                .font(FontId {
                                                    size: 11.0,
                                                    family: FontFamily::Monospace,
                                                })
                                                .color(FG_DIM),
                                        );
                                        ui.add_space(8.0);
                                        let text = egui::RichText::new(preview_text)
                                            .font(FontId {
                                                size: 12.0,
                                                family: FontFamily::Monospace,
                                            })
                                            .color(FG);
                                        ui.label(text);

                                        ui.with_layout(
                                            egui::Layout::right_to_left(egui::Align::Center),
                                            |ui| {
                                                let star =
                                                    if entry.favorite { "★" } else { "☆" };
                                                let star_color =
                                                    if entry.favorite { GREEN } else { FG_DIM };
                                                if ui
                                                    .add(
                                                        egui::Button::new(
                                                            egui::RichText::new(star)
                                                                .font(FontId {
                                                                    size: 13.0,
                                                                    family:
                                                                        FontFamily::Proportional,
                                                                })
                                                                .color(star_color),
                                                        )
                                                        .fill(Color32::TRANSPARENT)
                                                        .min_size(Vec2::new(24.0, 24.0)),
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
                                                ui.add_space(4.0);
                                            },
                                        );
                                        ui.add_space(4.0);
                                    });
                                })
                                .response;

                            if resp.clicked() {
                                let _ = set_clipboard(&entry.content);
                                should_close = true;
                            }

                            ui.add_space(2.0);
                        }
                    });

                ui.add_space(4.0);
            });

        if should_close || (self.hide_on_escape && ctx.input(|i| i.key_pressed(egui::Key::Escape)))
        {
            ctx.send_viewport_cmd(ViewportCommand::Close);
        }

        ctx.request_repaint_after(std::time::Duration::from_millis(500));
    }
}

fn set_clipboard(content: &str) -> anyhow::Result<()> {
    let mut child = std::process::Command::new("wl-copy")
        .stdin(std::process::Stdio::piped())
        .spawn()?;
    use std::io::Write;
    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(content.as_bytes())?;
    }
    child.wait()?;
    Ok(())
}
