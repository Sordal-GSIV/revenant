#[cfg(feature = "login-gui")]
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

    // ── Callbacks ─────────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_button_on_click_callback_fires() {
        use revenant::gui::GuiEvent;
        use std::sync::atomic::{AtomicBool, Ordering};

        let engine = make_engine();

        engine.eval_lua(r#"
            local btn = Gui.button("ok")
            btn:on_click(function(widget_id)
                _TEST_CALLBACK_FIRED = true
            end)
            _TEST_BTN_ID = btn._id
        "#).await.unwrap();

        let btn_id: u64 = engine.lua.globals().get::<u64>("_TEST_BTN_ID").unwrap();
        let win_id = 999u64;
        {
            let state = engine.gui_state.lock().unwrap();
            if let Some(tx) = &state.event_tx {
                tx.send(GuiEvent::ButtonClicked { window_id: win_id, widget_id: btn_id }).unwrap();
            }
        }

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let result: bool = engine.lua.globals().get("_TEST_CALLBACK_FIRED").unwrap_or(false);
        assert!(result, "on_click callback should have fired");
    }

    #[tokio::test]
    async fn test_window_on_close_callback_fires() {
        let engine = make_engine();
        engine.eval_lua(r#"
            local win, _ = Gui.window("Test", {})
            win:on_close(function()
                _TEST_CLOSE_FIRED = true
            end)
            _TEST_WIN_ID = win._id
        "#).await.unwrap();

        let win_id: u64 = engine.lua.globals().get::<u64>("_TEST_WIN_ID").unwrap();
        {
            let state = engine.gui_state.lock().unwrap();
            if let Some(tx) = &state.event_tx {
                tx.send(revenant::gui::GuiEvent::WindowClosed { window_id: win_id }).unwrap();
            }
        }

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let result: bool = engine.lua.globals().get("_TEST_CLOSE_FIRED").unwrap_or(false);
        assert!(result, "on_close callback should have fired");
    }

    // ── Gui.wait() ────────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_gui_wait_button_returns_on_event() {
        let engine = make_engine();

        engine.eval_lua(r#"
            local btn = Gui.button("ok")
            _TEST_BTN_ID = btn._id
            _TEST_WAIT_RESULT = nil
        "#).await.unwrap();

        let btn_id: u64 = engine.lua.globals().get::<u64>("_TEST_BTN_ID").unwrap();

        let (done_tx, done_rx) = tokio::sync::oneshot::channel::<()>();
        let engine2 = engine.clone();
        tokio::spawn(async move {
            engine2.eval_lua(r#"
                local btn2 = { _id = _TEST_BTN_ID, _type = "widget" }
                _TEST_WAIT_RESULT = Gui.wait(btn2, "click")
            "#).await.unwrap();
            let _ = done_tx.send(());
        });

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        {
            let state = engine.gui_state.lock().unwrap();
            if let Some(tx) = &state.event_tx {
                tx.send(revenant::gui::GuiEvent::ButtonClicked {
                    window_id: 0,
                    widget_id: btn_id,
                }).unwrap();
            }
        }

        tokio::time::timeout(std::time::Duration::from_millis(500), done_rx).await
            .expect("timed out waiting for Gui.wait to complete")
            .unwrap();

        let result: i64 = engine.lua.globals().get("_TEST_WAIT_RESULT").unwrap_or(0);
        assert_eq!(result, btn_id as i64);
    }

    #[tokio::test]
    async fn test_gui_wait_returns_nil_on_window_close() {
        let engine = make_engine();

        engine.eval_lua(r#"
            local win, _ = Gui.window("Test", {})
            local btn = Gui.button("ok")
            win:set_root(btn)
            _TEST_WIN_ID = win._id
            _TEST_BTN_ID = btn._id
            _TEST_WAIT_RESULT = "not_nil"
        "#).await.unwrap();

        let win_id: u64 = engine.lua.globals().get::<u64>("_TEST_WIN_ID").unwrap();

        let (done_tx, done_rx) = tokio::sync::oneshot::channel::<()>();
        let engine2 = engine.clone();
        tokio::spawn(async move {
            engine2.eval_lua(r#"
                local btn2 = { _id = _TEST_BTN_ID, _type = "widget" }
                local result, reason = Gui.wait(btn2, "click")
                _TEST_WAIT_RESULT = result
                _TEST_WAIT_REASON = reason
            "#).await.unwrap();
            let _ = done_tx.send(());
        });

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        {
            let state = engine.gui_state.lock().unwrap();
            if let Some(tx) = &state.event_tx {
                tx.send(revenant::gui::GuiEvent::WindowClosed { window_id: win_id }).unwrap();
            }
        }

        tokio::time::timeout(std::time::Duration::from_millis(500), done_rx).await
            .expect("timed out")
            .unwrap();

        let result: mlua::Value = engine.lua.globals().get("_TEST_WAIT_RESULT").unwrap();
        assert!(matches!(result, mlua::Value::Nil), "result should be nil on window close");

        let reason: String = engine.lua.globals().get("_TEST_WAIT_REASON").unwrap_or_default();
        assert_eq!(reason, "window closed");
    }
}
