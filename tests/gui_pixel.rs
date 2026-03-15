mod tests {
    use revenant::gui::map_view::{image_to_screen, pixel_to_room};
    use revenant::gui::{Marker, MarkerShape};
    use eframe::egui;

    fn rect(x: f32, y: f32, w: f32, h: f32) -> egui::Rect {
        egui::Rect::from_min_size(egui::pos2(x, y), egui::vec2(w, h))
    }

    #[test]
    fn test_image_to_screen_origin() {
        let r = rect(100.0, 50.0, 600.0, 400.0);
        let sp = image_to_screen((0.0, 0.0), r, 1.0);
        assert!((sp.x - 100.0).abs() < 0.1);
        assert!((sp.y - 50.0).abs() < 0.1);
    }

    #[test]
    fn test_image_to_screen_with_scale() {
        let r = rect(0.0, 0.0, 800.0, 600.0);
        let sp = image_to_screen((100.0, 50.0), r, 2.0);
        assert!((sp.x - 200.0).abs() < 0.1);
        assert!((sp.y - 100.0).abs() < 0.1);
    }

    #[test]
    fn test_pixel_to_room_nearest_within_threshold() {
        let markers = vec![
            Marker { room_id: 1, color: [1.0,0.0,0.0,1.0], shape: MarkerShape::Circle, position: (100.0, 100.0) },
            Marker { room_id: 2, color: [0.0,1.0,0.0,1.0], shape: MarkerShape::Circle, position: (200.0, 200.0) },
        ];
        let r = rect(0.0, 0.0, 800.0, 600.0);
        let click = egui::pos2(105.0, 98.0);
        let result = pixel_to_room(click, r, 1.0, &markers);
        assert_eq!(result, Some(1));
    }

    #[test]
    fn test_pixel_to_room_outside_threshold_returns_none() {
        let markers = vec![
            Marker { room_id: 1, color: [1.0,0.0,0.0,1.0], shape: MarkerShape::Circle, position: (100.0, 100.0) },
        ];
        let r = rect(0.0, 0.0, 800.0, 600.0);
        let click = egui::pos2(500.0, 500.0);
        let result = pixel_to_room(click, r, 1.0, &markers);
        assert_eq!(result, None);
    }

    #[test]
    fn test_pixel_to_room_picks_nearest_when_multiple_within_threshold() {
        let markers = vec![
            Marker { room_id: 10, color: [1.0,0.0,0.0,1.0], shape: MarkerShape::Circle, position: (100.0, 100.0) },
            Marker { room_id: 20, color: [0.0,0.0,1.0,1.0], shape: MarkerShape::Circle, position: (108.0, 100.0) },
        ];
        let r = rect(0.0, 0.0, 800.0, 600.0);
        let click = egui::pos2(107.0, 100.0);
        let result = pixel_to_room(click, r, 1.0, &markers);
        assert_eq!(result, Some(20));
    }

    #[test]
    fn test_pixel_to_room_exactly_at_threshold_returns_some() {
        let markers = vec![
            Marker { room_id: 5, color: [1.0,0.0,0.0,1.0], shape: MarkerShape::Circle, position: (100.0, 100.0) },
        ];
        let r = rect(0.0, 0.0, 800.0, 600.0);
        let click = egui::pos2(116.0, 100.0);
        let result = pixel_to_room(click, r, 1.0, &markers);
        assert_eq!(result, Some(5));
    }

    #[test]
    fn test_pixel_to_room_just_over_threshold_returns_none() {
        let markers = vec![
            Marker { room_id: 5, color: [1.0,0.0,0.0,1.0], shape: MarkerShape::Circle, position: (100.0, 100.0) },
        ];
        let r = rect(0.0, 0.0, 800.0, 600.0);
        let click = egui::pos2(116.01, 100.0);
        let result = pixel_to_room(click, r, 1.0, &markers);
        assert_eq!(result, None);
    }

    #[test]
    fn test_image_to_screen_nonzero_rect_min() {
        let r = rect(100.0, 50.0, 600.0, 400.0);
        let sp = image_to_screen((0.0, 0.0), r, 1.0);
        assert!((sp.x - 100.0).abs() < 0.1);
        assert!((sp.y - 50.0).abs() < 0.1);

        let sp2 = image_to_screen((10.0, 20.0), r, 2.0);
        assert!((sp2.x - 120.0).abs() < 0.1);
        assert!((sp2.y - 90.0).abs() < 0.1);
    }
}
