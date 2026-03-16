pub mod lua_api;
pub mod renderer;
pub mod map_view;

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::sync::mpsc::UnboundedSender;
use mlua::prelude::*;
use eframe::egui;

// ── IDs ──────────────────────────────────────────────────────────────────────

pub type WindowId = u64;
pub type WidgetId = u64;

static NEXT_WINDOW_ID: AtomicU64 = AtomicU64::new(1);
static NEXT_WIDGET_ID: AtomicU64 = AtomicU64::new(1);

pub fn next_window_id() -> WindowId { NEXT_WINDOW_ID.fetch_add(1, Ordering::Relaxed) }
pub fn next_widget_id() -> WidgetId { NEXT_WIDGET_ID.fetch_add(1, Ordering::Relaxed) }

// ── GuiState ─────────────────────────────────────────────────────────────────

pub struct GuiState {
    /// Windows indexed by WindowId.
    pub windows: HashMap<WindowId, WindowDef>,
    /// Flat map of widget properties, mutable after creation.
    pub widgets: HashMap<WidgetId, WidgetData>,
    /// Parent→children relationships for container widgets (VBox, HBox, Scroll).
    pub children: HashMap<WidgetId, Vec<WidgetId>>,
    /// Which window owns each widget (populated by win:set_root() tree walk).
    pub widget_window: HashMap<WidgetId, WindowId>,
    /// PNG bytes decoded to ColorImage on tokio thread; drained by renderer on egui thread.
    pub pending_textures: Vec<(String, egui::ColorImage)>,
    /// Send side of the event channel. Set by register_gui(); read by renderer.
    pub event_tx: Option<UnboundedSender<GuiEvent>>,
    /// Set true when any mutation occurs; renderer clears after processing.
    pub dirty: bool,
    /// Snapshot of the active egui_theme palette, set by the renderer each frame.
    pub palette_snapshot: Option<egui_theme::ColorPalette>,
}

impl Default for GuiState {
    fn default() -> Self {
        Self {
            windows:          HashMap::new(),
            widgets:          HashMap::new(),
            children:         HashMap::new(),
            widget_window:    HashMap::new(),
            pending_textures:  Vec::new(),
            event_tx:          None,
            dirty:             false,
            palette_snapshot:  None,
        }
    }
}

// ── WindowDef ─────────────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct WindowDef {
    pub title:       String,
    pub size:        (f32, f32),
    pub resizable:   bool,
    pub visible:     bool,
    /// Entry point into the widget tree; set by win:set_root().
    pub root_widget_id: Option<WidgetId>,
    /// Assigned at Gui.window() time via next_window_id() cast to ViewportId.
    pub viewport_id: egui::ViewportId,
}

// ── WidgetData ────────────────────────────────────────────────────────────────

/// Mutable per-widget properties. Stored flat in GuiState.widgets.
/// Structural relationships (children) are in GuiState.children.
#[derive(Clone)]
pub enum WidgetData {
    Label    { text: String },
    Button   { label: String },
    Checkbox { label: String, checked: bool },
    Input    { text: String, placeholder: String },
    Progress { value: f32 },
    Separator,
    Table    { columns: Vec<String>, rows: Vec<Vec<String>> },
    MapView  {
        image_path:    Option<String>,
        markers:       Vec<Marker>,
        scale:         f32,
        scroll_offset: (f32, f32),
    },
    VBox,
    HBox,
    Scroll,
    Badge         { text: String, color: String, outlined: bool },
    Card          { title: Option<String> },
    SectionHeader { text: String },
    Metric        { label: String, value: String, unit: Option<String>, trend: Option<f32>, icon: Option<char> },
    Toggle        { label: Option<String>, checked: bool },
    TabBar        { tabs: Vec<String>, selected: usize },
}

// ── Marker ───────────────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct Marker {
    pub room_id:  u32,
    pub color:    [f32; 4],   // RGBA straight alpha
    pub shape:    MarkerShape,
    pub position: (f32, f32), // image-space pixel coords (derived from image_coords)
}

#[derive(Clone, PartialEq)]
pub enum MarkerShape { Circle, X }

// ── GuiEvent ─────────────────────────────────────────────────────────────────

#[derive(Clone, Debug)]
pub enum GuiEvent {
    ButtonClicked   { window_id: WindowId, widget_id: WidgetId },
    CheckboxChanged { window_id: WindowId, widget_id: WidgetId, value: bool },
    InputChanged    { window_id: WindowId, widget_id: WidgetId, text: String },
    InputSubmitted  { window_id: WindowId, widget_id: WidgetId, text: String },
    MapClicked          { window_id: WindowId, widget_id: WidgetId, room_id: u32 },
    TableRowSelected    { window_id: WindowId, widget_id: WidgetId, row_index: usize },
    TabChanged          { window_id: WindowId, widget_id: WidgetId, index: usize },
    WindowClosed        { window_id: WindowId },
}

impl GuiEvent {
    /// Returns the WidgetId for widget-level events; None for WindowClosed.
    pub fn widget_id(&self) -> Option<WidgetId> {
        match self {
            GuiEvent::ButtonClicked   { widget_id, .. } => Some(*widget_id),
            GuiEvent::CheckboxChanged { widget_id, .. } => Some(*widget_id),
            GuiEvent::InputChanged    { widget_id, .. } => Some(*widget_id),
            GuiEvent::InputSubmitted  { widget_id, .. } => Some(*widget_id),
            GuiEvent::MapClicked          { widget_id, .. } => Some(*widget_id),
            GuiEvent::TableRowSelected    { widget_id, .. } => Some(*widget_id),
            GuiEvent::TabChanged          { widget_id, .. } => Some(*widget_id),
            GuiEvent::WindowClosed        { .. }            => None,
        }
    }

    /// Returns the WindowId for all events.
    pub fn window_id(&self) -> WindowId {
        match self {
            GuiEvent::ButtonClicked   { window_id, .. } => *window_id,
            GuiEvent::CheckboxChanged { window_id, .. } => *window_id,
            GuiEvent::InputChanged    { window_id, .. } => *window_id,
            GuiEvent::InputSubmitted  { window_id, .. } => *window_id,
            GuiEvent::MapClicked          { window_id, .. } => *window_id,
            GuiEvent::TableRowSelected    { window_id, .. } => *window_id,
            GuiEvent::TabChanged          { window_id, .. } => *window_id,
            GuiEvent::WindowClosed        { window_id }     => *window_id,
        }
    }

    /// Returns the WaitEventType for widget-level events; None for WindowClosed.
    pub fn wait_event_type(&self) -> Option<WaitEventType> {
        match self {
            GuiEvent::ButtonClicked   { .. } => Some(WaitEventType::Click),
            GuiEvent::MapClicked          { .. } => Some(WaitEventType::Click),
            GuiEvent::TableRowSelected    { .. } => Some(WaitEventType::Click),
            GuiEvent::CheckboxChanged     { .. } => Some(WaitEventType::Change),
            GuiEvent::InputChanged    { .. } => Some(WaitEventType::Change),
            GuiEvent::InputSubmitted  { .. } => Some(WaitEventType::Submit),
            GuiEvent::TabChanged      { .. } => Some(WaitEventType::Change),
            GuiEvent::WindowClosed    { .. } => None,
        }
    }

    /// Converts to a Lua value for delivery to callbacks/waiters:
    ///   ButtonClicked   → integer (widget_id)
    ///   CheckboxChanged → boolean (new value)
    ///   InputChanged    → string (current text)
    ///   InputSubmitted  → string (submitted text)
    ///   MapClicked      → integer (room_id)
    ///   WindowClosed    → nil
    pub fn into_lua_value(&self, lua: &Lua) -> LuaResult<LuaValue> {
        match self {
            GuiEvent::ButtonClicked   { widget_id, .. } => Ok(LuaValue::Integer(*widget_id as i64)),
            GuiEvent::CheckboxChanged { value, .. }     => Ok(LuaValue::Boolean(*value)),
            GuiEvent::InputChanged    { text, .. }      => Ok(LuaValue::String(lua.create_string(text)?)),
            GuiEvent::InputSubmitted  { text, .. }      => Ok(LuaValue::String(lua.create_string(text)?)),
            GuiEvent::MapClicked          { room_id, .. }    => Ok(LuaValue::Integer(*room_id as i64)),
            GuiEvent::TableRowSelected    { row_index, .. }  => Ok(LuaValue::Integer(*row_index as i64)),
            GuiEvent::TabChanged          { index, .. }      => Ok(LuaValue::Integer(*index as i64)),
            GuiEvent::WindowClosed        { .. }             => Ok(LuaValue::Nil),
        }
    }
}

// ── WaitKey ──────────────────────────────────────────────────────────────────

#[derive(Hash, Eq, PartialEq, Clone, Debug)]
pub enum WaitEventType { Click, Change, Submit }

/// Identifies what a Gui.wait() call is waiting for.
#[derive(Hash, Eq, PartialEq, Clone, Debug)]
pub enum WaitKey {
    /// Wait for a specific widget event: Gui.wait(widget, "click"|"change"|"submit")
    Widget     { widget_id: WidgetId, event_type: WaitEventType },
    /// Wait for any button click in a window: Gui.wait(win, "click")
    WindowClick { window_id: WindowId },
    /// Wait for window close: Gui.wait(win, "close")
    WindowClose { window_id: WindowId },
}
