mod tests {
    use revenant::gui::*;

    #[test]
    fn test_widget_ids_are_globally_unique() {
        let a = next_widget_id();
        let b = next_widget_id();
        let c = next_widget_id();
        assert_ne!(a, b);
        assert_ne!(b, c);
    }

    #[test]
    fn test_window_ids_are_globally_unique() {
        let a = next_window_id();
        let b = next_window_id();
        assert_ne!(a, b);
    }

    #[test]
    fn test_gui_event_widget_id() {
        let e = GuiEvent::ButtonClicked { window_id: 1, widget_id: 42 };
        assert_eq!(e.widget_id(), Some(42));

        let e = GuiEvent::CheckboxChanged { window_id: 1, widget_id: 7, value: true };
        assert_eq!(e.widget_id(), Some(7));

        let e = GuiEvent::WindowClosed { window_id: 1 };
        assert_eq!(e.widget_id(), None);
    }

    #[test]
    fn test_gui_event_window_id() {
        let e = GuiEvent::ButtonClicked { window_id: 99, widget_id: 1 };
        assert_eq!(e.window_id(), 99);

        let e = GuiEvent::WindowClosed { window_id: 55 };
        assert_eq!(e.window_id(), 55);
    }

    #[test]
    fn test_gui_event_wait_event_type() {
        assert_eq!(
            GuiEvent::ButtonClicked { window_id: 1, widget_id: 1 }.wait_event_type(),
            Some(WaitEventType::Click)
        );
        assert_eq!(
            GuiEvent::MapClicked { window_id: 1, widget_id: 1, room_id: 5 }.wait_event_type(),
            Some(WaitEventType::Click)
        );
        assert_eq!(
            GuiEvent::CheckboxChanged { window_id: 1, widget_id: 1, value: true }.wait_event_type(),
            Some(WaitEventType::Change)
        );
        assert_eq!(
            GuiEvent::InputChanged { window_id: 1, widget_id: 1, text: String::new() }.wait_event_type(),
            Some(WaitEventType::Change)
        );
        assert_eq!(
            GuiEvent::InputSubmitted { window_id: 1, widget_id: 1, text: String::new() }.wait_event_type(),
            Some(WaitEventType::Submit)
        );
        assert_eq!(
            GuiEvent::WindowClosed { window_id: 1 }.wait_event_type(),
            None
        );
    }

    #[test]
    fn test_gui_state_default_is_empty() {
        let s = GuiState::default();
        assert!(s.windows.is_empty());
        assert!(s.widgets.is_empty());
        assert!(s.children.is_empty());
        assert!(s.widget_window.is_empty());
    }
}
