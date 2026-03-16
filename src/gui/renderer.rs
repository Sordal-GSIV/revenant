use crate::gui::{GuiEvent, GuiState, WidgetData, WidgetId, WindowId};
use crate::gui::map_view;
use eframe::egui;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc::UnboundedSender;
use egui_theme;

// ── TextureCache ──────────────────────────────────────────────────────────────

/// Owned by the egui main thread. Never accessed from tokio threads.
pub type TextureCache = HashMap<String, egui::TextureHandle>;

// ── Renderer ─────────────────────────────────────────────────────────────────

pub struct Renderer {
    state:         Arc<Mutex<GuiState>>,
    texture_cache: TextureCache,
}

impl Renderer {
    pub fn new(state: Arc<Mutex<GuiState>>) -> Self {
        Self { state, texture_cache: HashMap::new() }
    }

    /// Called each egui frame from MonitorApp::update().
    pub fn render_frame(&mut self, ctx: &egui::Context) {
        // 1. Drain pending texture uploads
        let uploads: Vec<(String, egui::ColorImage)> = {
            let mut s = self.state.lock().unwrap();
            std::mem::take(&mut s.pending_textures)
        };
        for (path, image) in uploads {
            let handle = ctx.load_texture(&path, image, egui::TextureOptions::default());
            self.texture_cache.insert(path, handle);
        }

        // 2. Snapshot visible windows (minimise lock hold time)
        let windows: Vec<(WindowId, crate::gui::WindowDef)> = {
            let s = self.state.lock().unwrap();
            s.windows.iter()
                .filter(|(_, w)| w.visible)
                .map(|(id, w)| (*id, w.clone()))
                .collect()
        };

        // 3. Render each window as a separate egui viewport
        for (win_id, win_def) in windows {
            let state = self.state.clone();
            let tx: Option<UnboundedSender<GuiEvent>> = {
                state.lock().unwrap().event_tx.clone()
            };
            let Some(tx) = tx else { continue };
            let texture_cache = &self.texture_cache;

            ctx.show_viewport_immediate(
                win_def.viewport_id,
                egui::ViewportBuilder::default()
                    .with_title(&win_def.title)
                    .with_inner_size(egui::vec2(win_def.size.0, win_def.size.1))
                    .with_resizable(win_def.resizable),
                move |ctx, _class| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        if let Some(root_id) = win_def.root_widget_id {
                            let s = state.lock().unwrap();
                            render_widget(ui, root_id, &s, win_id, &tx, texture_cache);
                        }
                    });
                    // Detect close button
                    if ctx.input(|i| i.viewport().close_requested()) {
                        tx.send(GuiEvent::WindowClosed { window_id: win_id }).ok();
                    }
                },
            );
        }
    }
}

// ── Widget render traversal ───────────────────────────────────────────────────

fn render_widget(
    ui: &mut egui::Ui,
    widget_id: WidgetId,
    state: &GuiState,
    win_id: WindowId,
    tx: &UnboundedSender<GuiEvent>,
    texture_cache: &TextureCache,
) {
    let Some(data) = state.widgets.get(&widget_id) else { return };
    match data {
        WidgetData::Label { text } => {
            ui.label(text);
        }
        WidgetData::Button { label } => {
            if ui.button(label).clicked() {
                tx.send(GuiEvent::ButtonClicked { window_id: win_id, widget_id }).ok();
            }
        }
        WidgetData::Checkbox { label, checked } => {
            let mut v = *checked;
            if ui.checkbox(&mut v, label).changed() {
                tx.send(GuiEvent::CheckboxChanged { window_id: win_id, widget_id, value: v }).ok();
            }
        }
        WidgetData::Input { text, placeholder } => {
            let mut buf = text.clone();
            let resp = ui.add(
                egui::TextEdit::singleline(&mut buf).hint_text(placeholder)
            );
            if resp.changed() {
                tx.send(GuiEvent::InputChanged {
                    window_id: win_id, widget_id, text: buf.clone()
                }).ok();
            }
            if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                tx.send(GuiEvent::InputSubmitted {
                    window_id: win_id, widget_id, text: buf
                }).ok();
            }
        }
        WidgetData::Progress { value } => {
            egui_theme::ProgressBar::new(*value).show(ui);
        }
        WidgetData::Separator => {
            egui_theme::ThemedSeparator::fade().show(ui);
        }
        WidgetData::Table { columns, rows } => {
            egui::Grid::new(widget_id).show(ui, |ui| {
                for col in columns.iter() { ui.label(col); }
                ui.end_row();
                for row in rows.iter() {
                    for cell in row.iter() { ui.label(cell); }
                    ui.end_row();
                }
            });
        }
        WidgetData::MapView { .. } => {
            map_view::render(ui, widget_id, state, win_id, tx, texture_cache);
        }
        WidgetData::VBox => {
            ui.vertical(|ui| {
                if let Some(kids) = state.children.get(&widget_id) {
                    for kid in kids.iter() {
                        render_widget(ui, *kid, state, win_id, tx, texture_cache);
                    }
                }
            });
        }
        WidgetData::HBox => {
            ui.horizontal(|ui| {
                if let Some(kids) = state.children.get(&widget_id) {
                    for kid in kids.iter() {
                        render_widget(ui, *kid, state, win_id, tx, texture_cache);
                    }
                }
            });
        }
        WidgetData::Scroll => {
            egui::ScrollArea::vertical().id_salt(widget_id).show(ui, |ui| {
                if let Some(kids) = state.children.get(&widget_id) {
                    for kid in kids.iter() {
                        render_widget(ui, *kid, state, win_id, tx, texture_cache);
                    }
                }
            });
        }
        WidgetData::Badge { text, color, outlined } => {
            let badge_color = match color.to_lowercase().as_str() {
                "success"  => egui_theme::BadgeColor::Success,
                "error"    => egui_theme::BadgeColor::Error,
                "warning"  => egui_theme::BadgeColor::Warning,
                "info"     => egui_theme::BadgeColor::Info,
                _          => egui_theme::BadgeColor::Primary,
            };
            let mut badge = egui_theme::Badge::new(text).color(badge_color);
            if *outlined {
                badge = badge.outlined();
            }
            if badge.show(ui).clicked() {
                tx.send(GuiEvent::ButtonClicked { window_id: win_id, widget_id }).ok();
            }
        }
        WidgetData::Card { title } => {
            let card = match title {
                Some(t) => egui_theme::Card::new().with_title(t),
                None    => egui_theme::Card::new(),
            };
            card.show(ui, |ui| {
                if let Some(kids) = state.children.get(&widget_id) {
                    for kid in kids.iter() {
                        render_widget(ui, *kid, state, win_id, tx, texture_cache);
                    }
                }
            });
        }
        WidgetData::SectionHeader { text } => {
            egui_theme::SectionHeader::new(text).show(ui);
        }
        WidgetData::Metric { label, value, unit, trend, icon } => {
            let mut m = egui_theme::Metric::new(label, value);
            if let Some(u) = unit {
                m = m.unit(u);
            }
            if let Some(t) = trend {
                let trend_val = if *t > 0.0 {
                    egui_theme::Trend::Up
                } else if *t < 0.0 {
                    egui_theme::Trend::Down
                } else {
                    egui_theme::Trend::Neutral
                };
                m = m.trend(trend_val);
            }
            if let Some(ch) = icon {
                m = m.icon(*ch);
            }
            m.show(ui);
        }
        WidgetData::Toggle { label, checked } => {
            let mut v = *checked;
            let mut toggle = egui_theme::Toggle::new(&mut v);
            if let Some(lbl) = label {
                toggle = toggle.label(lbl);
            }
            if toggle.show(ui).changed() {
                tx.send(GuiEvent::CheckboxChanged { window_id: win_id, widget_id, value: v }).ok();
            }
        }
        WidgetData::TabBar { tabs, selected } => {
            let mut sel = *selected;
            let tab_strs: Vec<&str> = tabs.iter().map(|s| s.as_str()).collect();
            if egui_theme::TabBar::new(&mut sel, &tab_strs).show(ui).changed() {
                tx.send(GuiEvent::TabChanged { window_id: win_id, widget_id, index: sel }).ok();
            }
            // Render children for the selected tab
            if let Some(kids) = state.children.get(&widget_id) {
                if let Some(kid) = kids.get(*selected) {
                    render_widget(ui, *kid, state, win_id, tx, texture_cache);
                }
            }
        }
    }
}
