#![cfg(feature = "monitor")]

use crate::gui::*;
use crate::script_engine::ScriptEngine;
use mlua::prelude::*;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::sync::{mpsc, oneshot};

// ── Callback state (tokio thread only) ───────────────────────────────────────

pub struct GuiCallbacks {
    pub widget_callbacks:  HashMap<WidgetId, LuaFunction>,
    pub submit_callbacks:  HashMap<WidgetId, LuaFunction>, // InputSubmitted events
    pub window_callbacks:  HashMap<WindowId, LuaFunction>,
    pub waiters: HashMap<WaitKey, oneshot::Sender<Option<LuaValue>>>,
}

impl GuiCallbacks {
    pub fn new() -> Self {
        Self {
            widget_callbacks:  HashMap::new(),
            submit_callbacks:  HashMap::new(),
            window_callbacks:  HashMap::new(),
            waiters:           HashMap::new(),
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
    t.set("close", lua.create_function(move |_, _self: LuaValue| {
        gs2.lock().unwrap().windows.remove(&win_id);
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
            gs2.lock().unwrap().widgets.insert(id, WidgetData::Scroll);
            gs2.lock().unwrap().children.insert(id, vec![child_id]);
            make_base_widget(lua, id, gs2.clone())
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
                                    let color_image = eframe::egui::ColorImage { size: [w as usize, h as usize], pixels };
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

// ── Gui.wait() (Task 5) ───────────────────────────────────────────────────────
fn register_wait(_lua: &mlua::Lua, _gui: &LuaTable, _cbs: Arc<Mutex<GuiCallbacks>>) -> LuaResult<()> {
    // Populated in Task 5
    Ok(())
}

// ── gui_event_loop (Task 4) ───────────────────────────────────────────────────
async fn gui_event_loop(
    mut event_rx: tokio::sync::mpsc::UnboundedReceiver<GuiEvent>,
    _gs: Arc<Mutex<GuiState>>,
    _cbs: Arc<Mutex<GuiCallbacks>>,
    _lua: Arc<mlua::Lua>,
) {
    // Populated in Task 4; for now just drain the channel.
    while let Some(_event) = event_rx.recv().await {}
}
