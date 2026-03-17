use crate::gui::*;
use crate::script_engine::ScriptEngine;
use mlua::prelude::*;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::sync::{mpsc, oneshot};

// ── Callback state (tokio thread only) ───────────────────────────────────────

pub struct GuiCallbacks {
    pub widget_callbacks:       HashMap<WidgetId, LuaFunction>,
    pub submit_callbacks:       HashMap<WidgetId, LuaFunction>, // InputSubmitted events
    pub window_callbacks:       HashMap<WindowId, LuaFunction>,
    pub double_click_callbacks: HashMap<WidgetId, LuaFunction>,
    pub waiters: HashMap<WaitKey, oneshot::Sender<Option<LuaValue>>>,
}

impl GuiCallbacks {
    pub fn new() -> Self {
        Self {
            widget_callbacks:       HashMap::new(),
            submit_callbacks:       HashMap::new(),
            window_callbacks:       HashMap::new(),
            double_click_callbacks: HashMap::new(),
            waiters:                HashMap::new(),
        }
    }
}

// ── Registration entry point ──────────────────────────────────────────────────

pub fn register(engine: &ScriptEngine) -> LuaResult<()> {
    let lua = &engine.lua;
    let gui_state = engine.gui_state.clone();

    // Event channel: sender stored in GuiState (used by renderer),
    // receiver consumed by gui_event_loop task.
    let (event_tx, event_rx) = mpsc::unbounded_channel::<GuiEvent>();
    gui_state.lock().unwrap().event_tx = Some(event_tx);

    // Shared callback state lives on the tokio side.
    let callbacks = Arc::new(Mutex::new(GuiCallbacks::new()));

    // Spawn the event dispatch loop.
    let cbs = callbacks.clone();
    let gs  = gui_state.clone();
    let lua_arc = engine.lua.clone();
    tokio::spawn(gui_event_loop(event_rx, gs, cbs, lua_arc));

    // Build the Gui global table.
    let gui_table = lua.create_table()?;

    register_window_ctor(lua, &gui_table, gui_state.clone(), callbacks.clone())?;
    register_widget_ctors(lua, &gui_table, gui_state.clone(), callbacks.clone())?;
    register_map_view_ctor(lua, &gui_table, gui_state.clone(), callbacks.clone())?;
    register_advanced_widget_ctors(lua, &gui_table, gui_state.clone(), callbacks.clone())?;
    register_palette(lua, &gui_table, gui_state.clone())?;
    register_wait(lua, &gui_table, callbacks.clone())?;

    lua.globals().set("Gui", gui_table)?;
    Ok(())
}

// ── Gui.window() ─────────────────────────────────────────────────────────────

fn register_window_ctor(
    lua: &mlua::Lua,
    gui: &LuaTable,
    gs: Arc<Mutex<GuiState>>,
    cbs: Arc<Mutex<GuiCallbacks>>,
) -> LuaResult<()> {
    let gs2 = gs.clone();
    let cbs2 = cbs.clone();
    gui.set("window", lua.create_function(move |lua, (title, opts): (String, LuaTable)| {
        let width: f32  = opts.get("width").unwrap_or(400.0);
        let height: f32 = opts.get("height").unwrap_or(300.0);
        let resizable: bool = opts.get("resizable").unwrap_or(true);

        let win_id = next_window_id();
        let viewport_id = eframe::egui::ViewportId::from_hash_of(win_id);

        let def = WindowDef {
            title:          title,
            size:           (width, height),
            resizable,
            visible:        false,
            root_widget_id: None,
            viewport_id,
        };
        gs2.lock().unwrap().windows.insert(win_id, def);

        let tbl = make_window_table(lua, win_id, gs2.clone(), cbs2.clone())?;
        Ok(LuaMultiValue::from_vec(vec![LuaValue::Table(tbl), LuaValue::Nil]))
    })?)?;
    Ok(())
}

fn make_window_table(
    lua: &mlua::Lua,
    win_id: WindowId,
    gs: Arc<Mutex<GuiState>>,
    cbs: Arc<Mutex<GuiCallbacks>>,
) -> LuaResult<LuaTable> {
    let t = lua.create_table()?;
    t.set("_id", win_id)?;
    t.set("_type", "window")?;

    // show()
    let gs2 = gs.clone();
    t.set("show", lua.create_function(move |_, _self: LuaValue| {
        if let Some(w) = gs2.lock().unwrap().windows.get_mut(&win_id) { w.visible = true; }
        Ok(())
    })?)?;

    // hide()
    let gs2 = gs.clone();
    t.set("hide", lua.create_function(move |_, _self: LuaValue| {
        if let Some(w) = gs2.lock().unwrap().windows.get_mut(&win_id) { w.visible = false; }
        Ok(())
    })?)?;

    // close()
    let gs2 = gs.clone();
    let cbs2 = cbs.clone();
    t.set("close", lua.create_function(move |_, _self: LuaValue| {
        let mut state = gs2.lock().unwrap();
        // Collect widget IDs owned by this window
        let widget_ids: Vec<WidgetId> = state.widget_window.iter()
            .filter(|(_, wid)| **wid == win_id)
            .map(|(id, _)| *id)
            .collect();
        // Clean up widget data
        for wid in &widget_ids {
            state.widgets.remove(wid);
            state.children.remove(wid);
        }
        state.widget_window.retain(|_, wid| *wid != win_id);
        state.windows.remove(&win_id);
        drop(state);
        // Clean up callbacks
        cbs2.lock().unwrap().window_callbacks.remove(&win_id);
        Ok(())
    })?)?;

    // set_title(title)
    let gs2 = gs.clone();
    t.set("set_title", lua.create_function(move |_, (_self, title): (LuaValue, String)| {
        if let Some(w) = gs2.lock().unwrap().windows.get_mut(&win_id) { w.title = title; }
        Ok(())
    })?)?;

    // set_root(widget_table)
    let gs2 = gs.clone();
    t.set("set_root", lua.create_function(move |_, (_self, widget): (LuaValue, LuaTable)| {
        let widget_id: WidgetId = widget.get("_id")?;
        let mut state = gs2.lock().unwrap();
        if let Some(w) = state.windows.get_mut(&win_id) {
            w.root_widget_id = Some(widget_id);
        }
        walk_widget_window(&state.children.clone(), widget_id, win_id, &mut state.widget_window);
        state.dirty = true;
        Ok(())
    })?)?;

    // on_close(callback)
    let cbs2 = cbs.clone();
    t.set("on_close", lua.create_function(move |_, (_self, cb): (LuaValue, LuaFunction)| {
        cbs2.lock().unwrap().window_callbacks.insert(win_id, cb);
        Ok(())
    })?)?;

    Ok(t)
}

/// Walk the children tree and register widget→window mappings.
fn walk_widget_window(
    children: &HashMap<WidgetId, Vec<WidgetId>>,
    widget_id: WidgetId,
    win_id: WindowId,
    out: &mut HashMap<WidgetId, WindowId>,
) {
    out.insert(widget_id, win_id);
    if let Some(kids) = children.get(&widget_id) {
        for kid in kids {
            walk_widget_window(children, *kid, win_id, out);
        }
    }
}

// ── Widget constructors ───────────────────────────────────────────────────────

fn register_widget_ctors(
    lua: &mlua::Lua,
    gui: &LuaTable,
    gs: Arc<Mutex<GuiState>>,
    cbs: Arc<Mutex<GuiCallbacks>>,
) -> LuaResult<()> {
    // Gui.label(text)
    {
        let gs2 = gs.clone();
        gui.set("label", lua.create_function(move |lua, text: String| {
            let id = next_widget_id();
            gs2.lock().unwrap().widgets.insert(id, WidgetData::Label { text: text.clone() });
            let t = make_base_widget(lua, id, gs2.clone())?;

            let gs3 = gs2.clone();
            t.set("set_text", lua.create_function(move |_, (_self, text): (LuaValue, String)| {
                let mut s = gs3.lock().unwrap();
                if let Some(WidgetData::Label { text: ref mut v }) = s.widgets.get_mut(&id) {
                    *v = text;
                    s.dirty = true;
                }
                Ok(())
            })?)?;
            Ok(t)
        })?)?;
    }

    // Gui.button(label)
    {
        let gs2 = gs.clone();
        let cbs2 = cbs.clone();
        gui.set("button", lua.create_function(move |lua, label: String| {
            let id = next_widget_id();
            gs2.lock().unwrap().widgets.insert(id, WidgetData::Button { label });
            let t = make_base_widget(lua, id, gs2.clone())?;

            let gs3 = gs2.clone();
            t.set("set_text", lua.create_function(move |_, (_self, text): (LuaValue, String)| {
                let mut s = gs3.lock().unwrap();
                if let Some(WidgetData::Button { label: ref mut v }) = s.widgets.get_mut(&id) {
                    *v = text;
                    s.dirty = true;
                }
                Ok(())
            })?)?;

            add_on_click(lua, &t, id, cbs2.clone())?;
            Ok(t)
        })?)?;
    }

    // Gui.checkbox(label, checked)
    {
        let gs2 = gs.clone();
        let cbs2 = cbs.clone();
        gui.set("checkbox", lua.create_function(move |lua, (label, checked): (String, bool)| {
            let id = next_widget_id();
            gs2.lock().unwrap().widgets.insert(id, WidgetData::Checkbox { label, checked });
            let t = make_base_widget(lua, id, gs2.clone())?;

            let gs3 = gs2.clone();
            t.set("set_checked", lua.create_function(move |_, (_self, v): (LuaValue, bool)| {
                let mut s = gs3.lock().unwrap();
                if let Some(WidgetData::Checkbox { checked, .. }) = s.widgets.get_mut(&id) {
                    *checked = v;
                    s.dirty = true;
                }
                Ok(())
            })?)?;

            let gs3 = gs2.clone();
            t.set("get_checked", lua.create_function(move |_, _self: LuaValue| {
                let state = gs3.lock().unwrap();
                if let Some(WidgetData::Checkbox { checked, .. }) = state.widgets.get(&id) {
                    Ok(*checked)
                } else {
                    Ok(false)
                }
            })?)?;

            let cbs3 = cbs2.clone();
            t.set("on_change", lua.create_function(move |_, (_self, cb): (LuaValue, LuaFunction)| {
                cbs3.lock().unwrap().widget_callbacks.insert(id, cb);
                Ok(())
            })?)?;

            Ok(t)
        })?)?;
    }

    // Gui.input({ placeholder, text })
    {
        let gs2 = gs.clone();
        let cbs2 = cbs.clone();
        gui.set("input", lua.create_function(move |lua, opts: LuaTable| {
            let placeholder: String = opts.get("placeholder").unwrap_or_default();
            let text: String = opts.get("text").unwrap_or_default();
            let id = next_widget_id();
            gs2.lock().unwrap().widgets.insert(id, WidgetData::Input { text: text.clone(), placeholder });
            let t = make_base_widget(lua, id, gs2.clone())?;

            let gs3 = gs2.clone();
            t.set("set_text", lua.create_function(move |_, (_self, v): (LuaValue, String)| {
                let mut s = gs3.lock().unwrap();
                if let Some(WidgetData::Input { text, .. }) = s.widgets.get_mut(&id) {
                    *text = v;
                    s.dirty = true;
                }
                Ok(())
            })?)?;

            let gs3 = gs2.clone();
            t.set("get_text", lua.create_function(move |_, _self: LuaValue| {
                let state = gs3.lock().unwrap();
                if let Some(WidgetData::Input { text, .. }) = state.widgets.get(&id) {
                    Ok(text.clone())
                } else {
                    Ok(String::new())
                }
            })?)?;

            let cbs3 = cbs2.clone();
            t.set("on_change", lua.create_function(move |_, (_self, cb): (LuaValue, LuaFunction)| {
                cbs3.lock().unwrap().widget_callbacks.insert(id, cb);
                Ok(())
            })?)?;

            let cbs3 = cbs2.clone();
            t.set("on_submit", lua.create_function(move |_, (_self, cb): (LuaValue, LuaFunction)| {
                cbs3.lock().unwrap().submit_callbacks.insert(id, cb);
                Ok(())
            })?)?;

            Ok(t)
        })?)?;
    }

    // Gui.progress(value)
    {
        let gs2 = gs.clone();
        gui.set("progress", lua.create_function(move |lua, value: f32| {
            let id = next_widget_id();
            gs2.lock().unwrap().widgets.insert(id, WidgetData::Progress { value });
            let t = make_base_widget(lua, id, gs2.clone())?;

            let gs3 = gs2.clone();
            t.set("set_value", lua.create_function(move |_, (_self, v): (LuaValue, f32)| {
                let mut s = gs3.lock().unwrap();
                if let Some(WidgetData::Progress { value }) = s.widgets.get_mut(&id) {
                    *value = v.clamp(0.0, 1.0);
                    s.dirty = true;
                }
                Ok(())
            })?)?;

            Ok(t)
        })?)?;
    }

    // Gui.separator()
    {
        let gs2 = gs.clone();
        gui.set("separator", lua.create_function(move |lua, ()| {
            let id = next_widget_id();
            gs2.lock().unwrap().widgets.insert(id, WidgetData::Separator);
            make_base_widget(lua, id, gs2.clone())
        })?)?;
    }

    // Gui.table({ columns })
    {
        let gs2 = gs.clone();
        gui.set("table", lua.create_function(move |lua, opts: LuaTable| {
            let cols_lua: LuaTable = opts.get("columns")?;
            let columns: Vec<String> = (1..=cols_lua.len()?)
                .map(|i| cols_lua.get::<String>(i))
                .collect::<LuaResult<Vec<_>>>()?;
            let id = next_widget_id();
            gs2.lock().unwrap().widgets.insert(id, WidgetData::Table { columns, rows: Vec::new() });
            let t = make_base_widget(lua, id, gs2.clone())?;

            let gs3 = gs2.clone();
            t.set("add_row", lua.create_function(move |_, (_self, row): (LuaValue, LuaTable)| {
                let cells: Vec<String> = (1..=row.len()?)
                    .map(|i| row.get::<String>(i))
                    .collect::<LuaResult<Vec<_>>>()?;
                let mut s = gs3.lock().unwrap();
                if let Some(WidgetData::Table { rows, .. }) = s.widgets.get_mut(&id) {
                    rows.push(cells);
                    s.dirty = true;
                }
                Ok(())
            })?)?;

            let gs3 = gs2.clone();
            t.set("clear", lua.create_function(move |_, _self: LuaValue| {
                let mut s = gs3.lock().unwrap();
                if let Some(WidgetData::Table { rows, .. }) = s.widgets.get_mut(&id) {
                    rows.clear();
                    s.dirty = true;
                }
                Ok(())
            })?)?;

            Ok(t)
        })?)?;
    }

    // Gui.vbox()
    {
        let gs2 = gs.clone();
        gui.set("vbox", lua.create_function(move |lua, ()| {
            let id = next_widget_id();
            {
                let mut s = gs2.lock().unwrap();
                s.widgets.insert(id, WidgetData::VBox);
                s.children.insert(id, Vec::new());
            }
            let t = make_base_widget(lua, id, gs2.clone())?;
            add_add_method(lua, &t, id, gs2.clone())?;
            Ok(t)
        })?)?;
    }

    // Gui.hbox()
    {
        let gs2 = gs.clone();
        gui.set("hbox", lua.create_function(move |lua, ()| {
            let id = next_widget_id();
            {
                let mut s = gs2.lock().unwrap();
                s.widgets.insert(id, WidgetData::HBox);
                s.children.insert(id, Vec::new());
            }
            let t = make_base_widget(lua, id, gs2.clone())?;
            add_add_method(lua, &t, id, gs2.clone())?;
            Ok(t)
        })?)?;
    }

    // Gui.scroll(child)
    {
        let gs2 = gs.clone();
        gui.set("scroll", lua.create_function(move |lua, child: LuaTable| {
            let child_id: WidgetId = child.get("_id")?;
            let id = next_widget_id();
            {
                let mut s = gs2.lock().unwrap();
                s.widgets.insert(id, WidgetData::Scroll);
                s.children.insert(id, vec![child_id]);
            }
            make_base_widget(lua, id, gs2.clone())
        })?)?;
    }

    // Gui.badge(text, opts)
    {
        let gs2 = gs.clone();
        let cbs2 = cbs.clone();
        gui.set("badge", lua.create_function(move |lua, (text, opts): (String, Option<LuaTable>)| {
            let color: String = opts.as_ref().and_then(|o| o.get("color").ok()).unwrap_or_else(|| "accent".to_string());
            let outlined: bool = opts.as_ref().and_then(|o| o.get("outlined").ok()).unwrap_or(false);
            let id = next_widget_id();
            gs2.lock().unwrap().widgets.insert(id, WidgetData::Badge { text, color, outlined });
            let t = make_base_widget(lua, id, gs2.clone())?;
            add_on_click(lua, &t, id, cbs2.clone())?;
            Ok(t)
        })?)?;
    }

    // Gui.card(opts)
    {
        let gs2 = gs.clone();
        gui.set("card", lua.create_function(move |lua, opts: Option<LuaTable>| {
            let title: Option<String> = opts.as_ref().and_then(|o| o.get("title").ok());
            let id = next_widget_id();
            {
                let mut s = gs2.lock().unwrap();
                s.widgets.insert(id, WidgetData::Card { title });
                s.children.insert(id, Vec::new());
            }
            let t = make_base_widget(lua, id, gs2.clone())?;
            add_add_method(lua, &t, id, gs2.clone())?;
            Ok(t)
        })?)?;
    }

    // Gui.section_header(text)
    {
        let gs2 = gs.clone();
        gui.set("section_header", lua.create_function(move |lua, text: String| {
            let id = next_widget_id();
            gs2.lock().unwrap().widgets.insert(id, WidgetData::SectionHeader { text });
            make_base_widget(lua, id, gs2.clone())
        })?)?;
    }

    // Gui.metric(label, value, opts)
    {
        let gs2 = gs.clone();
        gui.set("metric", lua.create_function(move |lua, (label, value, opts): (String, String, Option<LuaTable>)| {
            let unit: Option<String> = opts.as_ref().and_then(|o| o.get("unit").ok());
            let trend: Option<f32> = opts.as_ref().and_then(|o| o.get("trend").ok());
            let icon: Option<char> = opts.as_ref()
                .and_then(|o| o.get::<String>("icon").ok())
                .and_then(|s| s.chars().next());
            let id = next_widget_id();
            gs2.lock().unwrap().widgets.insert(id, WidgetData::Metric { label, value, unit, trend, icon });
            make_base_widget(lua, id, gs2.clone())
        })?)?;
    }

    // Gui.toggle(label, checked)
    {
        let gs2 = gs.clone();
        let cbs2 = cbs.clone();
        gui.set("toggle", lua.create_function(move |lua, (label, checked): (Option<String>, bool)| {
            let id = next_widget_id();
            gs2.lock().unwrap().widgets.insert(id, WidgetData::Toggle { label, checked });
            let t = make_base_widget(lua, id, gs2.clone())?;

            let gs3 = gs2.clone();
            t.set("set_checked", lua.create_function(move |_, (_self, v): (LuaValue, bool)| {
                let mut s = gs3.lock().unwrap();
                if let Some(WidgetData::Toggle { checked, .. }) = s.widgets.get_mut(&id) {
                    *checked = v;
                    s.dirty = true;
                }
                Ok(())
            })?)?;

            let gs3 = gs2.clone();
            t.set("get_checked", lua.create_function(move |_, _self: LuaValue| {
                let state = gs3.lock().unwrap();
                if let Some(WidgetData::Toggle { checked, .. }) = state.widgets.get(&id) {
                    Ok(*checked)
                } else {
                    Ok(false)
                }
            })?)?;

            let cbs3 = cbs2.clone();
            t.set("on_change", lua.create_function(move |_, (_self, cb): (LuaValue, LuaFunction)| {
                cbs3.lock().unwrap().widget_callbacks.insert(id, cb);
                Ok(())
            })?)?;

            Ok(t)
        })?)?;
    }

    // Gui.tab_bar(tabs)
    {
        let gs2 = gs.clone();
        let cbs2 = cbs.clone();
        gui.set("tab_bar", lua.create_function(move |lua, tabs_lua: LuaTable| {
            let tabs: Vec<String> = (1..=tabs_lua.len()?)
                .map(|i| tabs_lua.get::<String>(i))
                .collect::<LuaResult<Vec<_>>>()?;
            let tab_count = tabs.len();
            let id = next_widget_id();
            {
                let mut s = gs2.lock().unwrap();
                s.widgets.insert(id, WidgetData::TabBar { tabs, selected: 0 });
                // One children slot per tab, all empty initially
                s.children.insert(id, vec![0; tab_count]);
            }
            let t = make_base_widget(lua, id, gs2.clone())?;

            // set_tab_content(index, widget) — 1-based index
            let gs3 = gs2.clone();
            t.set("set_tab_content", lua.create_function(move |_, (_self, index, child): (LuaValue, usize, LuaTable)| {
                let child_id: WidgetId = child.get("_id")?;
                let mut s = gs3.lock().unwrap();
                let children = s.children.entry(id).or_default();
                // Extend with zeros if needed (index is 1-based)
                if index > children.len() {
                    children.resize(index, 0);
                }
                children[index - 1] = child_id;
                s.dirty = true;
                Ok(())
            })?)?;

            let cbs3 = cbs2.clone();
            t.set("on_change", lua.create_function(move |_, (_self, cb): (LuaValue, LuaFunction)| {
                cbs3.lock().unwrap().widget_callbacks.insert(id, cb);
                Ok(())
            })?)?;

            Ok(t)
        })?)?;
    }

    Ok(())
}

// ── Helper: base widget table ─────────────────────────────────────────────────

fn make_base_widget(lua: &mlua::Lua, id: WidgetId, _gs: Arc<Mutex<GuiState>>) -> LuaResult<LuaTable> {
    let t = lua.create_table()?;
    t.set("_id", id)?;
    t.set("_type", "widget")?;
    Ok(t)
}

// ── Helper: add :add(child) method to containers ──────────────────────────────

fn add_add_method(lua: &mlua::Lua, t: &LuaTable, container_id: WidgetId, gs: Arc<Mutex<GuiState>>) -> LuaResult<()> {
    t.set("add", lua.create_function(move |_, (_self, child): (LuaValue, LuaTable)| {
        let child_id: WidgetId = child.get("_id")?;
        gs.lock().unwrap().children
            .entry(container_id).or_default()
            .push(child_id);
        Ok(())
    })?)
}

// ── Helper: add :on_click(cb) to clickable widgets ────────────────────────────

fn add_on_click(lua: &mlua::Lua, t: &LuaTable, id: WidgetId, cbs: Arc<Mutex<GuiCallbacks>>) -> LuaResult<()> {
    t.set("on_click", lua.create_function(move |_, (_self, cb): (LuaValue, LuaFunction)| {
        cbs.lock().unwrap().widget_callbacks.insert(id, cb);
        Ok(())
    })?)
}

// ── MapView constructor (Task 3) ──────────────────────────────────────────────

fn register_map_view_ctor(
    lua: &mlua::Lua,
    gui: &LuaTable,
    gs: Arc<Mutex<GuiState>>,
    cbs: Arc<Mutex<GuiCallbacks>>,
) -> LuaResult<()> {
    let gs2 = gs.clone();
    let cbs2 = cbs.clone();
    gui.set("map_view", lua.create_function(move |lua, opts: LuaTable| {
        let _width:  f32 = opts.get("width").unwrap_or(600.0);
        let _height: f32 = opts.get("height").unwrap_or(400.0);

        let id = next_widget_id();
        gs2.lock().unwrap().widgets.insert(id, WidgetData::MapView {
            image_path:    None,
            markers:       Vec::new(),
            scale:         1.0,
            scroll_offset: (0.0, 0.0),
        });

        let t = make_base_widget(lua, id, gs2.clone())?;

        // load_image(path) → (ok, err)
        {
            let gs3 = gs2.clone();
            t.set("load_image", lua.create_async_function(move |lua, path: String| {
                let gs4 = gs3.clone();
                async move {
                    if path.contains("..") {
                        return Ok(LuaMultiValue::from_vec(vec![
                            LuaValue::Nil,
                            LuaValue::String(lua.create_string("path traversal not allowed")?),
                        ]));
                    }
                    match tokio::fs::read(&path).await {
                        Err(e) => Ok(LuaMultiValue::from_vec(vec![
                            LuaValue::Nil,
                            LuaValue::String(lua.create_string(&e.to_string())?),
                        ])),
                        Ok(bytes) => {
                            match image::load_from_memory(&bytes) {
                                Err(e) => Ok(LuaMultiValue::from_vec(vec![
                                    LuaValue::Nil,
                                    LuaValue::String(lua.create_string(&e.to_string())?),
                                ])),
                                Ok(img) => {
                                    let rgba = img.to_rgba8();
                                    let (w, h) = rgba.dimensions();
                                    let pixels: Vec<eframe::egui::Color32> = rgba.pixels()
                                        .map(|p| eframe::egui::Color32::from_rgba_unmultiplied(p[0], p[1], p[2], p[3]))
                                        .collect();
                                    let color_image = eframe::egui::ColorImage { size: [w as usize, h as usize], pixels, source_size: eframe::egui::Vec2::new(w as f32, h as f32) };
                                    {
                                        let mut s = gs4.lock().unwrap();
                                        if let Some(WidgetData::MapView { image_path, .. }) = s.widgets.get_mut(&id) {
                                            *image_path = Some(path.clone());
                                        }
                                        s.pending_textures.push((path, color_image));
                                        s.dirty = true;
                                    }
                                    Ok(LuaMultiValue::from_vec(vec![LuaValue::Boolean(true), LuaValue::Nil]))
                                }
                            }
                        }
                    }
                }
            })?)?;
        }

        // set_marker(room_id, opts)
        {
            let gs3 = gs2.clone();
            t.set("set_marker", lua.create_function(move |_, (_self, room_id, opts): (LuaValue, u32, LuaTable)| {
                let color_str: String = opts.get("color").unwrap_or_else(|_| "white".to_string());
                let shape_str: String = opts.get("shape").unwrap_or_else(|_| "circle".to_string());
                let color = parse_color(&color_str);
                let shape = if shape_str == "x" { MarkerShape::X } else { MarkerShape::Circle };

                // room pixel position: real lookup requires map_data (post-implementation note).
                // Without map data loaded, silently skip.
                let position: Option<(f32, f32)> = None;
                let Some(position) = position else { return Ok(()); };

                let marker = Marker { room_id, color, shape, position };
                let mut s = gs3.lock().unwrap();
                if let Some(WidgetData::MapView { markers, .. }) = s.widgets.get_mut(&id) {
                    if let Some(m) = markers.iter_mut().find(|m| m.room_id == room_id) {
                        *m = marker;
                    } else {
                        markers.push(marker);
                    }
                    s.dirty = true;
                }
                Ok(())
            })?)?;
        }

        // clear_markers()
        {
            let gs3 = gs2.clone();
            t.set("clear_markers", lua.create_function(move |_, _self: LuaValue| {
                let mut s = gs3.lock().unwrap();
                if let Some(WidgetData::MapView { markers, .. }) = s.widgets.get_mut(&id) {
                    markers.clear();
                    s.dirty = true;
                }
                Ok(())
            })?)?;
        }

        // set_scale(factor) — clamped to [0.25, 4.0]
        {
            let gs3 = gs2.clone();
            t.set("set_scale", lua.create_function(move |_, (_self, factor): (LuaValue, f32)| {
                let mut s = gs3.lock().unwrap();
                if let Some(WidgetData::MapView { scale, .. }) = s.widgets.get_mut(&id) {
                    *scale = factor.clamp(0.25, 4.0);
                    s.dirty = true;
                }
                Ok(())
            })?)?;
        }

        // set_scroll_offset(x, y)
        {
            let gs3 = gs2.clone();
            t.set("set_scroll_offset", lua.create_function(move |_, (_self, x, y): (LuaValue, f32, f32)| {
                let mut s = gs3.lock().unwrap();
                if let Some(WidgetData::MapView { scroll_offset, .. }) = s.widgets.get_mut(&id) {
                    *scroll_offset = (x, y);
                    s.dirty = true;
                }
                Ok(())
            })?)?;
        }

        // center_on(room_id) — no-op until map data available (Task 6 follow-up)
        {
            t.set("center_on", lua.create_function(move |_, (_self, _room_id): (LuaValue, u32)| {
                Ok(())
            })?)?;
        }

        // on_click(callback)
        add_on_click(lua, &t, id, cbs2.clone())?;

        Ok(t)
    })?)?;
    Ok(())
}

// ── Advanced widget constructors (split_view, editable_combo, password_meter, side_tab_bar, tree_view) ──

fn register_advanced_widget_ctors(
    lua: &mlua::Lua,
    gui: &LuaTable,
    gs: Arc<Mutex<GuiState>>,
    cbs: Arc<Mutex<GuiCallbacks>>,
) -> LuaResult<()> {
    // Gui.split_view(opts)
    {
        let gs2 = gs.clone();
        gui.set("split_view", lua.create_function(move |lua, opts: Option<LuaTable>| {
            let direction: String = opts.as_ref().and_then(|o| o.get("direction").ok()).unwrap_or_else(|| "horizontal".to_string());
            let fraction: f32     = opts.as_ref().and_then(|o| o.get("fraction").ok()).unwrap_or(0.5);
            let min_frac: f32     = opts.as_ref().and_then(|o| o.get("min").ok()).unwrap_or(0.1);
            let max_frac: f32     = opts.as_ref().and_then(|o| o.get("max").ok()).unwrap_or(0.9);

            let id = next_widget_id();
            {
                let mut s = gs2.lock().unwrap();
                s.widgets.insert(id, WidgetData::SplitViewWidget { direction, fraction, min_frac, max_frac });
                s.children.insert(id, vec![0, 0]);
            }
            let t = make_base_widget(lua, id, gs2.clone())?;

            // set_first(widget)
            let gs3 = gs2.clone();
            t.set("set_first", lua.create_function(move |_, (_self, child): (LuaValue, LuaTable)| {
                let child_id: WidgetId = child.get("_id")?;
                let mut s = gs3.lock().unwrap();
                let kids = s.children.entry(id).or_insert_with(|| vec![0, 0]);
                if kids.is_empty() { kids.push(0); }
                kids[0] = child_id;
                s.dirty = true;
                Ok(())
            })?)?;

            // set_second(widget)
            let gs3 = gs2.clone();
            t.set("set_second", lua.create_function(move |_, (_self, child): (LuaValue, LuaTable)| {
                let child_id: WidgetId = child.get("_id")?;
                let mut s = gs3.lock().unwrap();
                let kids = s.children.entry(id).or_insert_with(|| vec![0, 0]);
                while kids.len() < 2 { kids.push(0); }
                kids[1] = child_id;
                s.dirty = true;
                Ok(())
            })?)?;

            Ok(t)
        })?)?;
    }

    // Gui.editable_combo(opts)
    {
        let gs2 = gs.clone();
        let cbs2 = cbs.clone();
        gui.set("editable_combo", lua.create_function(move |lua, opts: Option<LuaTable>| {
            let text: String    = opts.as_ref().and_then(|o| o.get("text").ok()).unwrap_or_default();
            let hint: String    = opts.as_ref().and_then(|o| o.get("hint").ok()).unwrap_or_default();
            let options: Vec<String> = opts.as_ref()
                .and_then(|o| o.get::<LuaTable>("options").ok())
                .map(|tbl| {
                    let len = tbl.len().unwrap_or(0);
                    (1..=len).filter_map(|i| tbl.get::<String>(i).ok()).collect()
                })
                .unwrap_or_default();

            let id = next_widget_id();
            gs2.lock().unwrap().widgets.insert(id, WidgetData::EditableCombo { text, options, hint });
            let t = make_base_widget(lua, id, gs2.clone())?;

            // on_change(fn)
            let cbs3 = cbs2.clone();
            t.set("on_change", lua.create_function(move |_, (_self, cb): (LuaValue, LuaFunction)| {
                cbs3.lock().unwrap().widget_callbacks.insert(id, cb);
                Ok(())
            })?)?;

            // get_text()
            let gs3 = gs2.clone();
            t.set("get_text", lua.create_function(move |_, _self: LuaValue| {
                let s = gs3.lock().unwrap();
                if let Some(WidgetData::EditableCombo { text, .. }) = s.widgets.get(&id) {
                    Ok(text.clone())
                } else {
                    Ok(String::new())
                }
            })?)?;

            // set_text(str)
            let gs3 = gs2.clone();
            t.set("set_text", lua.create_function(move |_, (_self, v): (LuaValue, String)| {
                let mut s = gs3.lock().unwrap();
                if let Some(WidgetData::EditableCombo { text, .. }) = s.widgets.get_mut(&id) {
                    *text = v;
                    s.dirty = true;
                }
                Ok(())
            })?)?;

            // set_options(opts)
            let gs3 = gs2.clone();
            t.set("set_options", lua.create_function(move |_, (_self, tbl): (LuaValue, LuaTable)| {
                let len = tbl.len()?;
                let new_opts: Vec<String> = (1..=len).filter_map(|i| tbl.get::<String>(i).ok()).collect();
                let mut s = gs3.lock().unwrap();
                if let Some(WidgetData::EditableCombo { options, .. }) = s.widgets.get_mut(&id) {
                    *options = new_opts;
                    s.dirty = true;
                }
                Ok(())
            })?)?;

            Ok(t)
        })?)?;
    }

    // Gui.password_meter()
    {
        let gs2 = gs.clone();
        gui.set("password_meter", lua.create_function(move |lua, ()| {
            let id = next_widget_id();
            gs2.lock().unwrap().widgets.insert(id, WidgetData::PasswordMeter { password: String::new() });
            let t = make_base_widget(lua, id, gs2.clone())?;

            // set_password(str)
            let gs3 = gs2.clone();
            t.set("set_password", lua.create_function(move |_, (_self, v): (LuaValue, String)| {
                let mut s = gs3.lock().unwrap();
                if let Some(WidgetData::PasswordMeter { password }) = s.widgets.get_mut(&id) {
                    *password = v;
                    s.dirty = true;
                }
                Ok(())
            })?)?;

            Ok(t)
        })?)?;
    }

    // Gui.side_tab_bar(tabs, opts)
    {
        let gs2 = gs.clone();
        let cbs2 = cbs.clone();
        gui.set("side_tab_bar", lua.create_function(move |lua, (tabs_lua, opts): (LuaTable, Option<LuaTable>)| {
            let tabs: Vec<String> = (1..=tabs_lua.len()?)
                .map(|i| tabs_lua.get::<String>(i))
                .collect::<LuaResult<Vec<_>>>()?;
            let tab_width: f32 = opts.as_ref().and_then(|o| o.get("tab_width").ok()).unwrap_or(120.0);
            let tab_count = tabs.len();
            let id = next_widget_id();
            {
                let mut s = gs2.lock().unwrap();
                s.widgets.insert(id, WidgetData::SideTabView { tabs, selected: 0, tab_width });
                s.children.insert(id, vec![0; tab_count]);
            }
            let t = make_base_widget(lua, id, gs2.clone())?;

            // set_tab_content(index, widget) — 1-based
            let gs3 = gs2.clone();
            t.set("set_tab_content", lua.create_function(move |_, (_self, index, child): (LuaValue, usize, LuaTable)| {
                let child_id: WidgetId = child.get("_id")?;
                let mut s = gs3.lock().unwrap();
                let children = s.children.entry(id).or_default();
                if index > children.len() {
                    children.resize(index, 0);
                }
                children[index - 1] = child_id;
                s.dirty = true;
                Ok(())
            })?)?;

            // on_change(fn)
            let cbs3 = cbs2.clone();
            t.set("on_change", lua.create_function(move |_, (_self, cb): (LuaValue, LuaFunction)| {
                cbs3.lock().unwrap().widget_callbacks.insert(id, cb);
                Ok(())
            })?)?;

            Ok(t)
        })?)?;
    }

    // Gui.tree_view(opts)
    {
        let gs2 = gs.clone();
        let cbs2 = cbs.clone();
        gui.set("tree_view", lua.create_function(move |lua, opts: LuaTable| {
            // Parse columns
            let cols_lua: LuaTable = opts.get("columns")?;
            let col_count = cols_lua.len()?;
            let mut columns: Vec<egui_theme::TreeColumn> = Vec::new();
            for i in 1..=col_count {
                let col_tbl: LuaTable = cols_lua.get(i)?;
                let label: String = col_tbl.get("label").unwrap_or_default();
                let width: Option<f32> = col_tbl.get("width").ok();
                let sortable: bool = col_tbl.get("sortable").unwrap_or(false);
                columns.push(egui_theme::TreeColumn { label, width, sortable });
            }

            // Parse rows recursively
            let rows_lua: LuaTable = opts.get("rows").unwrap_or_else(|_| lua.create_table().unwrap());
            let rows = parse_tree_rows(&rows_lua)?;

            let id = next_widget_id();
            gs2.lock().unwrap().widgets.insert(id, WidgetData::TreeViewWidget {
                columns,
                rows,
                selected: None,
                sort_column: None,
                sort_ascending: true,
            });
            let t = make_base_widget(lua, id, gs2.clone())?;

            // on_click(fn)
            let cbs3 = cbs2.clone();
            t.set("on_click", lua.create_function(move |_, (_self, cb): (LuaValue, LuaFunction)| {
                cbs3.lock().unwrap().widget_callbacks.insert(id, cb);
                Ok(())
            })?)?;

            // on_double_click(fn)
            let cbs3 = cbs2.clone();
            t.set("on_double_click", lua.create_function(move |_, (_self, cb): (LuaValue, LuaFunction)| {
                cbs3.lock().unwrap().double_click_callbacks.insert(id, cb);
                Ok(())
            })?)?;

            // set_rows(rows)
            let gs3 = gs2.clone();
            t.set("set_rows", lua.create_function(move |_, (_self, rows_tbl): (LuaValue, LuaTable)| {
                let new_rows = parse_tree_rows(&rows_tbl)?;
                let mut s = gs3.lock().unwrap();
                if let Some(WidgetData::TreeViewWidget { rows, selected, .. }) = s.widgets.get_mut(&id) {
                    *rows = new_rows;
                    *selected = None;
                    s.dirty = true;
                }
                Ok(())
            })?)?;

            // get_selected()
            let gs3 = gs2.clone();
            t.set("get_selected", lua.create_function(move |_, _self: LuaValue| {
                let s = gs3.lock().unwrap();
                if let Some(WidgetData::TreeViewWidget { selected, .. }) = s.widgets.get(&id) {
                    Ok(selected.map(|i| i as i64))
                } else {
                    Ok(None)
                }
            })?)?;

            Ok(t)
        })?)?;
    }

    Ok(())
}

/// Recursively parse a Lua table of tree rows into Vec<TreeRow>.
fn parse_tree_rows(tbl: &LuaTable) -> LuaResult<Vec<egui_theme::TreeRow>> {
    let len = tbl.len()?;
    let mut rows = Vec::new();
    for i in 1..=len {
        let row_tbl: LuaTable = tbl.get(i)?;

        // cells
        let cells: Vec<String> = if let Ok(cells_tbl) = row_tbl.get::<LuaTable>("cells") {
            let cell_count = cells_tbl.len()?;
            (1..=cell_count).filter_map(|j| cells_tbl.get::<String>(j).ok()).collect()
        } else {
            Vec::new()
        };

        // children (recursive)
        let children: Vec<egui_theme::TreeRow> = if let Ok(children_tbl) = row_tbl.get::<LuaTable>("children") {
            parse_tree_rows(&children_tbl)?
        } else {
            Vec::new()
        };

        let expanded: bool = row_tbl.get("expanded").unwrap_or(false);

        rows.push(egui_theme::TreeRow { cells, children, expanded });
    }
    Ok(rows)
}

fn parse_color(s: &str) -> [f32; 4] {
    match s {
        "red"    => [1.0, 0.0, 0.0, 1.0],
        "green"  => [0.0, 1.0, 0.0, 1.0],
        "blue"   => [0.0, 0.0, 1.0, 1.0],
        "yellow" => [1.0, 1.0, 0.0, 1.0],
        "white"  => [1.0, 1.0, 1.0, 1.0],
        "black"  => [0.0, 0.0, 0.0, 1.0],
        _        => [1.0, 1.0, 1.0, 1.0],
    }
}

// ── Gui.palette() ────────────────────────────────────────────────────────────

fn register_palette(
    lua: &mlua::Lua,
    gui: &LuaTable,
    gs: Arc<Mutex<GuiState>>,
) -> LuaResult<()> {
    let gs2 = gs.clone();
    gui.set("palette", lua.create_function(move |lua, ()| {
        let s = gs2.lock().unwrap();
        let palette = s.palette_snapshot.clone().unwrap_or_else(egui_theme::ColorPalette::slate);
        let t = lua.create_table()?;

        let c2t = |lua: &mlua::Lua, c: eframe::egui::Color32| -> LuaResult<LuaTable> {
            let t = lua.create_table()?;
            t.set("r", c.r())?;
            t.set("g", c.g())?;
            t.set("b", c.b())?;
            t.set("a", c.a())?;
            Ok(t)
        };

        t.set("base",          c2t(lua, palette.base)?)?;
        t.set("panel",         c2t(lua, palette.panel)?)?;
        t.set("surface",       c2t(lua, palette.surface)?)?;
        t.set("elevated",      c2t(lua, palette.elevated)?)?;
        t.set("accent",        c2t(lua, palette.accent)?)?;
        t.set("accent_hover",  c2t(lua, palette.accent_hover)?)?;
        t.set("success",       c2t(lua, palette.success)?)?;
        t.set("error",         c2t(lua, palette.error)?)?;
        t.set("warning",       c2t(lua, palette.warning)?)?;
        t.set("info",          c2t(lua, palette.info)?)?;
        t.set("text_primary",  c2t(lua, palette.text_primary)?)?;
        t.set("text_secondary",c2t(lua, palette.text_secondary)?)?;
        t.set("text_muted",    c2t(lua, palette.text_muted)?)?;
        t.set("border",        c2t(lua, palette.border)?)?;
        t.set("border_subtle", c2t(lua, palette.border_subtle)?)?;
        Ok(t)
    })?)?;
    Ok(())
}

// ── Gui.wait() (Task 5) ───────────────────────────────────────────────────────
fn register_wait(
    lua: &mlua::Lua,
    gui: &LuaTable,
    cbs: Arc<Mutex<GuiCallbacks>>,
) -> LuaResult<()> {
    gui.set("wait", lua.create_async_function(move |lua, (target, event_str): (LuaTable, String)| {
        let cbs2 = cbs.clone();
        async move {
            let target_type: String = target.get("_type").unwrap_or_default();
            let wait_key = if target_type == "window" {
                let win_id: WindowId = target.get("_id")?;
                match event_str.as_str() {
                    "close" => WaitKey::WindowClose { window_id: win_id },
                    "click" => WaitKey::WindowClick { window_id: win_id },
                    other   => return Err(mlua::Error::RuntimeError(
                        format!("Gui.wait: unknown event '{}' for window", other)
                    )),
                }
            } else {
                let widget_id: WidgetId = target.get("_id")?;
                let et = match event_str.as_str() {
                    "click"  => WaitEventType::Click,
                    "change" => WaitEventType::Change,
                    "submit" => WaitEventType::Submit,
                    other    => return Err(mlua::Error::RuntimeError(
                        format!("Gui.wait: unknown event '{}' for widget", other)
                    )),
                };
                WaitKey::Widget { widget_id, event_type: et }
            };

            let (tx, rx) = oneshot::channel::<Option<LuaValue>>();
            cbs2.lock().unwrap().waiters.insert(wait_key, tx);

            match rx.await {
                Ok(Some(val)) => Ok(LuaMultiValue::from_vec(vec![val, LuaValue::Nil])),
                Ok(None) => {
                    Ok(LuaMultiValue::from_vec(vec![
                        LuaValue::Nil,
                        LuaValue::String(lua.create_string("window closed")?),
                    ]))
                }
                Err(_) => {
                    Ok(LuaMultiValue::from_vec(vec![LuaValue::Nil, LuaValue::Nil]))
                }
            }
        }
    })?)?;
    Ok(())
}

// ── gui_event_loop (Task 4) ───────────────────────────────────────────────────
async fn gui_event_loop(
    mut event_rx: tokio::sync::mpsc::UnboundedReceiver<GuiEvent>,
    gs: Arc<Mutex<GuiState>>,
    cbs: Arc<Mutex<GuiCallbacks>>,
    lua: Arc<mlua::Lua>,
) {
    while let Some(event) = event_rx.recv().await {
        let _win_id = event.window_id();

        // ── WindowClosed: drain all waiters for this window ───────────────
        if let GuiEvent::WindowClosed { window_id } = &event {
            // Collect all widget IDs belonging to this window
            let widget_ids: Vec<WidgetId> = gs.lock().unwrap()
                .widget_window.iter()
                .filter(|(_, wid)| **wid == *window_id)
                .map(|(id, _)| *id)
                .collect();

            {
                let mut c = cbs.lock().unwrap();
                // Drain widget-scoped waiters
                for wid in &widget_ids {
                    for et in [WaitEventType::Click, WaitEventType::Change, WaitEventType::Submit] {
                        let key = WaitKey::Widget { widget_id: *wid, event_type: et };
                        if let Some(tx) = c.waiters.remove(&key) { tx.send(None).ok(); }
                    }
                }
                // Drain window-scoped waiters
                if let Some(tx) = c.waiters.remove(&WaitKey::WindowClick { window_id: *window_id }) {
                    tx.send(None).ok();
                }
                if let Some(tx) = c.waiters.remove(&WaitKey::WindowClose { window_id: *window_id }) {
                    tx.send(None).ok();
                }
            }

            // Fire on_close callback
            let cb = cbs.lock().unwrap().window_callbacks.get(window_id).cloned();
            if let Some(cb) = cb {
                let _ = cb.call_async::<()>(LuaValue::Nil).await;
            }
            continue;
        }

        // ── InputChanged: write text back to widget state ─────────────────
        if let GuiEvent::InputChanged { widget_id, ref text, .. } = event {
            let mut s = gs.lock().unwrap();
            match s.widgets.get_mut(&widget_id) {
                Some(WidgetData::Input { text: ref mut v, .. }) => {
                    *v = text.clone();
                    s.dirty = true;
                }
                Some(WidgetData::EditableCombo { text: ref mut v, .. }) => {
                    *v = text.clone();
                    s.dirty = true;
                }
                Some(WidgetData::SplitViewWidget { fraction, .. }) => {
                    if let Ok(f) = text.parse::<f32>() {
                        *fraction = f;
                        s.dirty = true;
                    }
                }
                _ => {}
            }
        }

        // ── Checkbox/Toggle state writeback ───────────────────────────────
        if let GuiEvent::CheckboxChanged { widget_id, value, .. } = &event {
            let mut s = gs.lock().unwrap();
            match s.widgets.get_mut(widget_id) {
                Some(WidgetData::Checkbox { checked, .. }) => { *checked = *value; s.dirty = true; }
                Some(WidgetData::Toggle   { checked, .. }) => { *checked = *value; s.dirty = true; }
                _ => {}
            }
        }

        // ── TabBar / SideTabView selected index writeback ─────────────────
        if let GuiEvent::TabChanged { widget_id, index, .. } = &event {
            let mut s = gs.lock().unwrap();
            match s.widgets.get_mut(widget_id) {
                Some(WidgetData::TabBar { selected, .. }) => {
                    *selected = *index;
                    s.dirty = true;
                }
                Some(WidgetData::SideTabView { selected, .. }) => {
                    *selected = *index;
                    s.dirty = true;
                }
                _ => {}
            }
        }

        // ── TreeRowClicked: update selected ──────────────────────────────
        if let GuiEvent::TreeRowClicked { widget_id, row_index, .. } = &event {
            let mut s = gs.lock().unwrap();
            if let Some(WidgetData::TreeViewWidget { selected, .. }) = s.widgets.get_mut(widget_id) {
                *selected = Some(*row_index);
                s.dirty = true;
            }
        }

        // ── TreeRowDoubleClicked: dispatch to double_click_callbacks ──────
        if let GuiEvent::TreeRowDoubleClicked { widget_id, row_index, .. } = &event {
            let cb = cbs.lock().unwrap().double_click_callbacks.get(widget_id).cloned();
            if let Some(cb) = cb {
                let _ = cb.call_async::<()>(LuaValue::Integer(*row_index as i64)).await;
            }
            continue;
        }

        // ── ButtonClicked: check widget waiter, then window-click waiter ──
        if let GuiEvent::ButtonClicked { window_id, widget_id } = &event {
            let wkey = WaitKey::Widget { widget_id: *widget_id, event_type: WaitEventType::Click };
            if let Some(tx) = cbs.lock().unwrap().waiters.remove(&wkey) {
                tx.send(Some(LuaValue::Integer(*widget_id as i64))).ok();
                continue;
            }
            let wkey = WaitKey::WindowClick { window_id: *window_id };
            if let Some(tx) = cbs.lock().unwrap().waiters.remove(&wkey) {
                tx.send(Some(LuaValue::Integer(*widget_id as i64))).ok();
                continue;
            }
        }

        // ── All other widget events: check widget waiter ──────────────────
        if let Some(widget_id) = event.widget_id() {
            if let Some(et) = event.wait_event_type() {
                let wkey = WaitKey::Widget { widget_id, event_type: et };
                if let Some(tx) = cbs.lock().unwrap().waiters.remove(&wkey) {
                    let lua_val = match &event {
                        GuiEvent::CheckboxChanged { value, .. } => LuaValue::Boolean(*value),
                        GuiEvent::InputChanged    { text, .. }  |
                        GuiEvent::InputSubmitted  { text, .. }  => lua.create_string(text.as_str())
                            .map(LuaValue::String)
                            .unwrap_or(LuaValue::Nil),
                        GuiEvent::MapClicked         { room_id, .. }   => LuaValue::Integer(*room_id as i64),
                        GuiEvent::TabChanged         { index, .. }     => LuaValue::Integer(*index as i64),
                        GuiEvent::TreeRowClicked     { row_index, .. } => LuaValue::Integer(*row_index as i64),
                        _ => LuaValue::Nil,
                    };
                    tx.send(Some(lua_val)).ok();
                    continue;
                }
            }
        }

        // ── No waiter matched — fire widget callback ──────────────────────
        if let Some(widget_id) = event.widget_id() {
            let cb = match &event {
                GuiEvent::InputSubmitted { .. } => {
                    cbs.lock().unwrap().submit_callbacks.get(&widget_id).cloned()
                }
                _ => {
                    cbs.lock().unwrap().widget_callbacks.get(&widget_id).cloned()
                }
            };
            if let Some(cb) = cb {
                let event_val = match &event {
                    GuiEvent::ButtonClicked      { widget_id, .. }  => LuaValue::Integer(*widget_id as i64),
                    GuiEvent::CheckboxChanged    { value, .. }      => LuaValue::Boolean(*value),
                    GuiEvent::InputChanged       { text, .. }       |
                    GuiEvent::InputSubmitted     { text, .. }       => lua.create_string(text.as_str())
                        .map(LuaValue::String)
                        .unwrap_or(LuaValue::Nil),
                    GuiEvent::MapClicked         { room_id, .. }    => LuaValue::Integer(*room_id as i64),
                    GuiEvent::TabChanged         { index, .. }      => LuaValue::Integer(*index as i64),
                    GuiEvent::TreeRowClicked     { row_index, .. }  => LuaValue::Integer(*row_index as i64),
                    _ => LuaValue::Nil,
                };
                let _ = cb.call_async::<()>(event_val).await;
            }
        }
    }
}
