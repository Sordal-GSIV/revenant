use crate::gui::{GuiEvent, GuiState, Marker, MarkerShape, WidgetData, WidgetId, WindowId};
use eframe::egui;
use std::collections::HashMap;
use tokio::sync::mpsc::UnboundedSender;

// ── Public pixel utilities (used by tests) ────────────────────────────────────

/// Convert image-space pixel position to screen position, given the rendered rect and scale.
pub fn image_to_screen(pos: (f32, f32), rect: egui::Rect, scale: f32) -> egui::Pos2 {
    rect.min + egui::vec2(pos.0 * scale, pos.1 * scale)
}

/// Find the nearest room to a screen click within a 16-pixel threshold.
/// Returns None if no marker is within the threshold.
pub fn pixel_to_room(
    screen_pos: egui::Pos2,
    rect: egui::Rect,
    scale: f32,
    markers: &[Marker],
) -> Option<u32> {
    const THRESHOLD: f32 = 16.0;
    markers.iter()
        .map(|m| {
            let sp = image_to_screen(m.position, rect, scale);
            let dist = (sp - screen_pos).length();
            (dist, m.room_id)
        })
        .filter(|(dist, _)| *dist <= THRESHOLD)
        .min_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal))
        .map(|(_, room_id)| room_id)
}

// ── MapView egui render ───────────────────────────────────────────────────────

pub fn render(
    ui: &mut egui::Ui,
    widget_id: WidgetId,
    state: &GuiState,
    win_id: WindowId,
    tx: &UnboundedSender<GuiEvent>,
    texture_cache: &HashMap<String, egui::TextureHandle>,
) {
    let Some(WidgetData::MapView { image_path, markers, scale, scroll_offset }) =
        state.widgets.get(&widget_id) else { return };

    let Some(path) = image_path else {
        ui.label("(no map loaded)");
        return;
    };

    let Some(texture) = texture_cache.get(path) else {
        ui.label("(loading...)");
        return;
    };

    let img_size = texture.size_vec2() * *scale;
    let markers = markers.clone();
    let scale = *scale;
    let scroll_offset = egui::vec2(scroll_offset.0, scroll_offset.1);

    egui::ScrollArea::both()
        .scroll_offset(scroll_offset)
        .drag_to_scroll(true)
        .id_salt(widget_id)
        .show(ui, |ui| {
            let response = ui.add(
                egui::Image::new(texture).fit_to_exact_size(img_size)
            );
            let painter = ui.painter_at(response.rect);

            for marker in &markers {
                let sp = image_to_screen(marker.position, response.rect, scale);
                let color = egui::Color32::from_rgba_unmultiplied(
                    (marker.color[0] * 255.0) as u8,
                    (marker.color[1] * 255.0) as u8,
                    (marker.color[2] * 255.0) as u8,
                    (marker.color[3] * 255.0) as u8,
                );
                let stroke = egui::Stroke::new(2.0, color);
                match marker.shape {
                    MarkerShape::Circle => {
                        painter.circle_stroke(sp, 6.0, stroke);
                    }
                    MarkerShape::X => {
                        let r = 5.0_f32;
                        painter.line_segment(
                            [sp + egui::vec2(-r, -r), sp + egui::vec2(r, r)],
                            stroke,
                        );
                        painter.line_segment(
                            [sp + egui::vec2(r, -r), sp + egui::vec2(-r, r)],
                            stroke,
                        );
                    }
                }
            }

            if response.clicked() {
                if let Some(click_pos) = response.interact_pointer_pos() {
                    if let Some(room_id) = pixel_to_room(click_pos, response.rect, scale, &markers) {
                        tx.send(GuiEvent::MapClicked {
                            window_id: win_id,
                            widget_id,
                            room_id,
                        }).ok();
                    }
                }
            }
        });
}
