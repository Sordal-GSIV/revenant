#[cfg(feature = "monitor")]
mod tests {
    use revenant::script_engine::ScriptEngine;
    use std::sync::Arc;

    fn make_engine() -> Arc<ScriptEngine> {
        let e = Arc::new(ScriptEngine::new());
        e.install_lua_api().unwrap();
        e
    }

    // ── Window management ─────────────────────────────────────────────────

    #[tokio::test]
    async fn test_gui_window_creates_window_in_state() {
        let engine = make_engine();
        engine.eval_lua(r#"
            local win, err = Gui.window("Test", { width=400, height=300 })
            assert(win ~= nil, tostring(err))
        "#).await.unwrap();
        assert_eq!(engine.gui_state.lock().unwrap().windows.len(), 1);
    }

    #[tokio::test]
    async fn test_gui_window_show_sets_visible() {
        let engine = make_engine();
        engine.eval_lua(r#"
            local win, err = Gui.window("Test", {})
            assert(win ~= nil, tostring(err))
            win:show()
        "#).await.unwrap();
        let state = engine.gui_state.lock().unwrap();
        let win = state.windows.values().next().unwrap();
        assert!(win.visible);
    }

    #[tokio::test]
    async fn test_gui_window_hide_sets_not_visible() {
        let engine = make_engine();
        engine.eval_lua(r#"
            local win, err = Gui.window("Test", {})
            win:show()
            win:hide()
        "#).await.unwrap();
        let state = engine.gui_state.lock().unwrap();
        let win = state.windows.values().next().unwrap();
        assert!(!win.visible);
    }

    #[tokio::test]
    async fn test_gui_window_set_title() {
        let engine = make_engine();
        engine.eval_lua(r#"
            local win, err = Gui.window("Old Title", {})
            win:set_title("New Title")
        "#).await.unwrap();
        let state = engine.gui_state.lock().unwrap();
        let win = state.windows.values().next().unwrap();
        assert_eq!(win.title, "New Title");
    }

    #[tokio::test]
    async fn test_gui_window_close_removes_window() {
        let engine = make_engine();
        engine.eval_lua(r#"
            local win, err = Gui.window("Test", {})
            win:close()
        "#).await.unwrap();
        assert_eq!(engine.gui_state.lock().unwrap().windows.len(), 0);
    }

    // ── Widget constructors ───────────────────────────────────────────────

    #[tokio::test]
    async fn test_gui_label_creates_widget() {
        let engine = make_engine();
        engine.eval_lua(r#"
            local lbl = Gui.label("hello")
            assert(lbl ~= nil)
            assert(lbl._id ~= nil)
        "#).await.unwrap();
        assert_eq!(engine.gui_state.lock().unwrap().widgets.len(), 1);
    }

    #[tokio::test]
    async fn test_gui_button_creates_widget() {
        let engine = make_engine();
        engine.eval_lua(r#"
            local btn = Gui.button("Click me")
            assert(btn ~= nil)
        "#).await.unwrap();
        assert_eq!(engine.gui_state.lock().unwrap().widgets.len(), 1);
    }

    #[tokio::test]
    async fn test_gui_vbox_add_records_children() {
        let engine = make_engine();
        engine.eval_lua(r#"
            local vbox = Gui.vbox()
            local lbl  = Gui.label("hello")
            local btn  = Gui.button("ok")
            vbox:add(lbl)
            vbox:add(btn)
        "#).await.unwrap();
        let state = engine.gui_state.lock().unwrap();
        assert_eq!(state.widgets.len(), 3);
        let vbox_id = state.widgets.iter()
            .find(|(_, v)| matches!(v, revenant::gui::WidgetData::VBox))
            .map(|(id, _)| *id)
            .unwrap();
        assert_eq!(state.children.get(&vbox_id).unwrap().len(), 2);
    }

    #[tokio::test]
    async fn test_gui_set_root_records_widget_window_map() {
        let engine = make_engine();
        engine.eval_lua(r#"
            local win, _ = Gui.window("Test", {})
            local btn = Gui.button("ok")
            win:set_root(btn)
        "#).await.unwrap();
        let state = engine.gui_state.lock().unwrap();
        assert_eq!(state.widget_window.len(), 1);
    }

    // ── Property mutation ─────────────────────────────────────────────────

    #[tokio::test]
    async fn test_gui_label_set_text() {
        let engine = make_engine();
        engine.eval_lua(r#"
            local lbl = Gui.label("old")
            lbl:set_text("new")
        "#).await.unwrap();
        let state = engine.gui_state.lock().unwrap();
        let data = state.widgets.values().next().unwrap();
        if let revenant::gui::WidgetData::Label { text } = data {
            assert_eq!(text, "new");
        } else {
            panic!("expected Label");
        }
    }

    #[tokio::test]
    async fn test_gui_progress_set_value() {
        let engine = make_engine();
        engine.eval_lua(r#"
            local p = Gui.progress(0.0)
            p:set_value(0.75)
        "#).await.unwrap();
        let state = engine.gui_state.lock().unwrap();
        let data = state.widgets.values().next().unwrap();
        if let revenant::gui::WidgetData::Progress { value } = data {
            assert!((value - 0.75).abs() < 0.001);
        } else {
            panic!("expected Progress");
        }
    }

    #[tokio::test]
    async fn test_gui_checkbox_get_set_checked() {
        let engine = make_engine();
        engine.eval_lua(r#"
            local chk = Gui.checkbox("Enable", false)
            assert(chk:get_checked() == false)
            chk:set_checked(true)
            assert(chk:get_checked() == true)
        "#).await.unwrap();
    }

    #[tokio::test]
    async fn test_gui_input_get_set_text() {
        let engine = make_engine();
        engine.eval_lua(r#"
            local inp = Gui.input({ placeholder="hint", text="init" })
            assert(inp:get_text() == "init")
            inp:set_text("updated")
            assert(inp:get_text() == "updated")
        "#).await.unwrap();
    }

    #[tokio::test]
    async fn test_gui_table_add_row_and_clear() {
        let engine = make_engine();
        engine.eval_lua(r#"
            local tbl = Gui.table({ columns = {"Name", "Value"} })
            tbl:add_row({ "health", "95" })
            tbl:add_row({ "mana", "50" })
            tbl:clear()
        "#).await.unwrap();
        let state = engine.gui_state.lock().unwrap();
        let data = state.widgets.values().next().unwrap();
        if let revenant::gui::WidgetData::Table { rows, .. } = data {
            assert!(rows.is_empty());
        } else {
            panic!("expected Table");
        }
    }

    // ── MapView ───────────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_gui_map_view_creates_widget() {
        let engine = make_engine();
        engine.eval_lua(r#"
            local m = Gui.map_view({ width=600, height=400 })
            assert(m ~= nil)
        "#).await.unwrap();
        assert_eq!(engine.gui_state.lock().unwrap().widgets.len(), 1);
    }

    #[tokio::test]
    async fn test_gui_map_view_set_marker_and_clear() {
        let engine = make_engine();
        engine.eval_lua(r#"
            local m = Gui.map_view({})
            m:set_marker(1234, { color="red", shape="circle" })
            m:clear_markers()
        "#).await.unwrap();
        let state = engine.gui_state.lock().unwrap();
        let data = state.widgets.values().next().unwrap();
        if let revenant::gui::WidgetData::MapView { markers, .. } = data {
            assert!(markers.is_empty());
        } else {
            panic!("expected MapView");
        }
    }

    #[tokio::test]
    async fn test_gui_map_view_set_scale_clamped() {
        let engine = make_engine();
        engine.eval_lua(r#"
            local m = Gui.map_view({})
            m:set_scale(10.0)  -- should clamp to 4.0
        "#).await.unwrap();
        let state = engine.gui_state.lock().unwrap();
        let data = state.widgets.values().next().unwrap();
        if let revenant::gui::WidgetData::MapView { scale, .. } = data {
            assert!(*scale <= 4.0 + f32::EPSILON);
        } else {
            panic!("expected MapView");
        }
    }

    #[tokio::test]
    async fn test_gui_map_view_set_scroll_offset() {
        let engine = make_engine();
        engine.eval_lua(r#"
            local m = Gui.map_view({})
            m:set_scroll_offset(100.0, 200.0)
        "#).await.unwrap();
        let state = engine.gui_state.lock().unwrap();
        let data = state.widgets.values().next().unwrap();
        if let revenant::gui::WidgetData::MapView { scroll_offset, .. } = data {
            assert!((scroll_offset.0 - 100.0).abs() < 0.1);
            assert!((scroll_offset.1 - 200.0).abs() < 0.1);
        } else {
            panic!("expected MapView");
        }
    }
}
