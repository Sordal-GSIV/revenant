#![cfg(feature = "monitor")]

use crate::gui::renderer::Renderer;
use crate::script_engine::ScriptEngine;
use eframe::egui;
use std::sync::Arc;

pub struct MonitorApp {
    engine:   Arc<ScriptEngine>,
    renderer: Renderer,
}

impl MonitorApp {
    pub fn new(engine: Arc<ScriptEngine>) -> Self {
        let renderer = Renderer::new(engine.gui_state.clone());
        Self { engine, renderer }
    }
}

impl eframe::App for MonitorApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
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
                    add_bar(ui, "❤", gs.health, gs.max_health, egui::Color32::from_rgb(180, 40, 40));
                    add_bar(ui, "✦", gs.mana, gs.max_mana, egui::Color32::from_rgb(60, 100, 200));
                    add_bar(ui, "☯", gs.spirit, gs.max_spirit, egui::Color32::from_rgb(150, 80, 200));
                    add_bar(ui, "⚡", gs.stamina, gs.max_stamina, egui::Color32::from_rgb(200, 140, 20));
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
                    indicator(ui, "BLEEDING", gs.bleeding, egui::Color32::RED);
                    indicator(ui, "STUNNED", gs.stunned, egui::Color32::YELLOW);
                    indicator(ui, "DEAD", gs.dead, egui::Color32::DARK_RED);
                    indicator(ui, "SLEEPING", gs.sleeping, egui::Color32::from_rgb(100, 100, 200));
                    indicator(ui, "PRONE", gs.prone, egui::Color32::GRAY);
                    indicator(ui, "SITTING", gs.sitting, egui::Color32::GRAY);
                    indicator(ui, "KNEELING", gs.kneeling, egui::Color32::GRAY);

                    let rt = gs.roundtime();
                    if rt > 0.0 {
                        ui.separator();
                        ui.label("RT:");
                        ui.add(egui::ProgressBar::new((rt / 10.0_f64).min(1.0) as f32)
                            .text(format!("{rt:.1}s"))
                            .desired_width(80.0)
                            .fill(egui::Color32::from_rgb(200, 100, 0)));
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
                                .color(egui::Color32::from_rgb(100, 220, 100))
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

fn indicator(ui: &mut egui::Ui, label: &str, active: bool, color: egui::Color32) {
    let text = egui::RichText::new(label)
        .color(if active { color } else { egui::Color32::DARK_GRAY })
        .strong();
    ui.label(text);
}
