#![cfg(feature = "monitor")]

use crate::gui::renderer::Renderer;
use crate::script_engine::ScriptEngine;
use eframe::egui;
use std::sync::Arc;

pub struct MonitorApp {
    engine:        Arc<ScriptEngine>,
    renderer:      Renderer,
    theme_applied: bool,
    theme_name:    String,
}

impl MonitorApp {
    pub fn new(engine: Arc<ScriptEngine>, theme_name: &str) -> Self {
        let renderer = Renderer::new(engine.gui_state.clone());
        Self { engine, renderer, theme_applied: false, theme_name: theme_name.to_string() }
    }
}

impl eframe::App for MonitorApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if !self.theme_applied {
            let config = crate::theme_config::ThemeConfig { theme: self.theme_name.clone() };
            config.to_theme().apply(ctx);
            self.theme_applied = true;
        }

        let palette = egui_theme::palette_from_ctx(ctx);
        self.engine.gui_state.lock().unwrap().palette_snapshot = Some(palette.clone());
        let style = egui_theme::style_from_ctx(ctx);
        let vitals = &style.vitals_colors;

        // Repaint every 250ms even with no input
        ctx.request_repaint_after(std::time::Duration::from_millis(250));

        // Attempt to read game state — may be None if no client connected yet
        let gs_arc = {
            let guard = self.engine.game_state.lock().unwrap();
            guard.as_ref().cloned()
        };

        // ── Vitals panel ──────────────────────────────────────────────────────
        egui::TopBottomPanel::top("vitals").show(ctx, |ui| {
            if let Some(ref gs_arc) = gs_arc {
                let gs = gs_arc.read().unwrap_or_else(|e| e.into_inner());
                ui.horizontal(|ui| {
                    add_bar(ui, "❤", gs.health, gs.max_health, vitals.health);
                    add_bar(ui, "✦", gs.mana, gs.max_mana, vitals.mana);
                    add_bar(ui, "☯", gs.spirit, gs.max_spirit, vitals.spirit);
                    add_bar(ui, "⚡", gs.stamina, gs.max_stamina, vitals.stamina);
                });
            } else {
                ui.label("No client connected");
            }
        });

        // ── Status indicators ────────────────────────────────────────────────
        egui::TopBottomPanel::top("indicators").show(ctx, |ui| {
            if let Some(ref gs_arc) = gs_arc {
                let gs = gs_arc.read().unwrap_or_else(|e| e.into_inner());
                ui.horizontal(|ui| {
                    indicator(ui, "BLEEDING", gs.bleeding, palette.error, palette.border);
                    indicator(ui, "STUNNED", gs.stunned, palette.warning, palette.border);
                    indicator(ui, "DEAD", gs.dead, vitals.dead, palette.border);
                    indicator(ui, "SLEEPING", gs.sleeping, palette.info, palette.border);
                    indicator(ui, "PRONE", gs.prone, palette.text_muted, palette.border);
                    indicator(ui, "SITTING", gs.sitting, palette.text_muted, palette.border);
                    indicator(ui, "KNEELING", gs.kneeling, palette.text_muted, palette.border);

                    let rt = gs.roundtime();
                    if rt > 0.0 {
                        ui.separator();
                        ui.label("RT:");
                        ui.add(egui::ProgressBar::new((rt / 10.0_f64).min(1.0) as f32)
                            .text(format!("{rt:.1}s"))
                            .desired_width(80.0)
                            .fill(palette.warning));
                    }
                });
            }
        });

        // ── Room panel ───────────────────────────────────────────────────────
        egui::TopBottomPanel::top("room").show(ctx, |ui| {
            if let Some(ref gs_arc) = gs_arc {
                let gs = gs_arc.read().unwrap_or_else(|e| e.into_inner());
                ui.horizontal(|ui| {
                    let room_label = if gs.room_name.is_empty() { "—".to_string() } else { gs.room_name.clone() };
                    let id_str = gs.room_id.map(|id| format!(" [#{id}]")).unwrap_or_default();
                    ui.strong(format!("{room_label}{id_str}"));
                    if !gs.room_exits.is_empty() {
                        ui.separator();
                        ui.label(format!("Exits: {}", gs.room_exits.join(", ")));
                    }
                });
            }
        });

        // ── Right panel: active spells + running scripts ──────────────────
        egui::SidePanel::right("side").min_width(160.0).show(ctx, |ui| {
            ui.heading("Spells");
            if let Some(ref gs_arc) = gs_arc {
                let gs = gs_arc.read().unwrap_or_else(|e| e.into_inner());
                if gs.active_spells.is_empty() {
                    ui.label("—");
                } else {
                    for spell in &gs.active_spells {
                        let dur = spell.duration_secs.map(|d| format!(" ({d}s)")).unwrap_or_default();
                        ui.label(format!("• {}{dur}", spell.name));
                    }
                }
            } else {
                ui.label("—");
            }

            ui.separator();
            ui.heading("Scripts");
            let running = self.engine.running.lock().unwrap();
            let active: Vec<_> = running.iter()
                .filter(|(_, h)| !h.is_finished())
                .map(|(n, _)| n.clone())
                .collect();
            drop(running);
            if active.is_empty() {
                ui.label("—");
            } else {
                for name in &active {
                    ui.label(format!("▶ {name}"));
                }
            }
        });

        // ── Main area: game text + respond() log ─────────────────────────
        egui::CentralPanel::default().show(ctx, |ui| {
            egui::ScrollArea::vertical()
                .auto_shrink([false; 2])
                .stick_to_bottom(true)
                .show(ui, |ui| {
                    let game: Vec<String> = self.engine.game_log.lock().unwrap().iter().cloned().collect();
                    let respond: Vec<String> = self.engine.respond_log.lock().unwrap().iter().cloned().collect();
                    for line in &game {
                        ui.monospace(line.as_str());
                    }
                    for line in &respond {
                        ui.monospace(
                            egui::RichText::new(line.as_str())
                                .color(palette.success)
                        );
                    }
                });
        });

        // Render any script-created GUI windows
        self.renderer.render_frame(ctx);
    }
}

fn add_bar(ui: &mut egui::Ui, icon: &str, value: u32, max: u32, color: egui::Color32) {
    ui.label(icon);
    let frac = if max > 0 { value as f32 / max as f32 } else { 0.0 };
    ui.add(egui::ProgressBar::new(frac)
        .text(format!("{value}/{max}"))
        .desired_width(120.0)
        .fill(color));
}

fn indicator(ui: &mut egui::Ui, label: &str, active: bool, color: egui::Color32, inactive_color: egui::Color32) {
    let text = egui::RichText::new(label)
        .color(if active { color } else { inactive_color })
        .strong();
    ui.label(text);
}
