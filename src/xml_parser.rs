use quick_xml::events::Event;
use quick_xml::Reader;
pub use crate::game_obj::ObjCategory;
use crate::game_state::Game;

/// Which hand slot an object update refers to.
#[derive(Debug, Clone, PartialEq)]
pub enum ObjHand { Right, Left }

#[derive(Debug, Clone, PartialEq)]
pub enum XmlEvent {
    Health { value: u32, max: Option<u32> },
    Mana { value: u32, max: Option<u32> },
    Spirit { value: u32, max: Option<u32> },
    Stamina { value: u32, max: Option<u32> },
    Concentration { value: u32, max: Option<u32> },
    RoundTime { end_epoch: i64 },
    CastTime { end_epoch: i64 },
    Prompt { time: i64, text: String },
    RoomName { name: String },
    RoomDescription { text: String },
    RoomExits { exits: Vec<String>, raw: String },
    RoomId { id: u32 },
    PreparedSpell { name: String },
    SpellCleared,
    ActiveSpell { name: String, duration: Option<u32> },
    Indicator { name: String, visible: bool },
    RightHand { item: Option<String> },
    LeftHand { item: Option<String> },
    Level { value: u32 },
    Experience { value: u64 },
    Text { content: String },
    StreamWindow { id: String, title: String },
    Mode { id: String, room_id: Option<u32> },
    /// <style id="roomName"/> … <style id=""/> styled-text room name/desc
    StylePush { id: String },
    StylePop,
    Unknown { tag: String },
    /// A game object was created/updated in a room or inventory stream.
    GameObjCreate {
        id: String,
        noun: String,
        name: String,
        category: ObjCategory,
        /// NPC/PC status string if present (e.g., "dead", "prone").
        status: Option<String>,
    },
    /// A component stream started sending fresh content — clear that registry.
    ComponentClear { component_id: String },
    /// The game sent a hand object with an existence ID.
    GameObjHandUpdate { hand: ObjHand, id: String, noun: String, name: String },
    /// The hand is empty ("nothing").
    GameObjHandClear { hand: ObjHand },
    /// An injury image tag inside <component id="injuries">.
    Injury {
        body_part: String,
        wound: u8,
        scar: u8,
    },
    /// A named stream was pushed onto the stream stack.
    PushStream { id: String },
    /// A named stream was popped from the stream stack.
    PopStream { id: String },
    /// A named stream was cleared (content reset).
    ClearStream { id: String },
    /// A dialog (Effects panel) was cleared.
    DialogClear { dialog_id: String },
    /// A single entry in an effects dialog (buff, debuff, cooldown, spell).
    DialogEntry { dialog_id: String, entry_id: String, name: String, duration_secs: u32 },
    /// Text emitted inside a named stream (bounty, society, etc.).
    StreamText { stream_id: String, text: String },
    /// DR dual-compass room count bump (second compass close = room transition).
    RoomCountBump,
    /// DR room ID extracted from streamWindow subtitle (does NOT increment room_count).
    RoomIdOnly { id: u32 },
    /// Clear all active spells (DR percWindow flush).
    ClearActiveSpells,
    /// Auto-detected game from settingsInfo instance attribute.
    GameDetected { game: Game },
    /// Room name inside the familiar's stream.
    FamiliarRoomName { name: String },
    /// Room description text inside the familiar's stream.
    FamiliarRoomDescription { text: String },
    /// Exits accumulated from the familiar stream compass/exit links.
    FamiliarRoomExits { exits: Vec<String> },
    /// A game object seen by the familiar (NPC, loot, PC, or room-desc object).
    FamiliarObjCreate {
        id: String,
        noun: String,
        name: String,
        category: ObjCategory,
    },
}

/// Familiar stream section — determines how `<a>` link objects are categorized.
#[derive(Debug, Clone, PartialEq, Default)]
enum FamMode {
    #[default]
    None,
    Things,  // "You also see..." — loot and NPCs
    People,  // "Also here:" — PCs
    Paths,   // "Obvious paths/exits:" — exits
}

/// Streaming GemStone XML parser.
///
/// GemStone sends a pseudo-XML byte stream over TCP. TCP read boundaries do not
/// align with XML element boundaries, so a tag or element may be split across
/// multiple reads. `StreamParser` accumulates data in an internal buffer and
/// only processes complete tokens — exactly like Lich5's `@buffer` approach.
///
/// Call `feed()` for each TCP read; it returns all fully-parseable events from
/// the buffer and leaves any incomplete token for the next call.
pub struct StreamParser {
    buf: String,
    current_style: Option<String>,
    /// Which `<component id="...">` stream is currently open, if any.
    current_component: Option<String>,
    /// Whether we are inside a `<b>` bold tag (distinguishes NPCs from loot).
    in_bold: bool,
    /// Container ID set by `<inv id="...">` tag; scopes subsequent `<a>` objects.
    current_inv_container: Option<String>,
    /// Which named stream (`<pushStream id="...">`) is currently active, if any.
    current_stream: Option<String>,
    /// State machine tracking which section of the familiar stream we are in.
    fam_mode: FamMode,
    /// Exit links accumulated while in `FamMode::Paths`; flushed on `popStream`.
    fam_exits: Vec<String>,
    /// Which `<dialogData id="...">` is currently open, if any.
    current_dialog: Option<String>,
    /// Which game we are parsing for (affects compass, percWindow, etc.).
    game: Game,
    /// DR: toggles on first `</compass>`, emits RoomCountBump on second.
    dr_second_compass: bool,
    /// DR: whether we are currently tracking percWindow spell lines.
    dr_perc_tracking: bool,
    /// DR: accumulated spell entries from percWindow text lines.
    dr_perc_spells: Vec<(String, Option<u32>)>,
}

impl StreamParser {
    pub fn new(game: Game) -> Self {
        Self {
            buf: String::new(),
            current_style: None,
            current_component: None,
            in_bold: false,
            current_inv_container: None,
            current_stream: None,
            fam_mode: FamMode::None,
            fam_exits: Vec::new(),
            current_dialog: None,
            game,
            dr_second_compass: false,
            dr_perc_tracking: false,
            dr_perc_spells: Vec::new(),
        }
    }

    /// Whether it is safe for scripts to inject `respond()` output to the client.
    /// In DR, injecting text while inside a stream or styled block corrupts the FE display.
    pub fn safe_to_respond(&self) -> bool {
        match self.game {
            Game::DragonRealms => self.current_stream.is_none() && !self.in_bold && self.current_style.is_none(),
            Game::GemStone => true,
        }
    }

    /// Append `data` to the internal buffer and return all complete events.
    pub fn feed(&mut self, data: &str) -> Vec<XmlEvent> {
        self.buf.push_str(data);
        self.drain()
    }

    fn drain(&mut self) -> Vec<XmlEvent> {
        let mut events = Vec::new();

        loop {
            match self.buf.find('<') {
                None => {
                    // No tag start — all plain text
                    let text = std::mem::take(&mut self.buf);
                    self.emit_text(&mut events, &text);
                    break;
                }
                Some(0) => {
                    // Buffer starts with a tag; find its closing '>'
                    match find_gt(&self.buf, 0) {
                        None => break, // Tag incomplete — wait for more data
                        Some(gt) => {
                            let tag_inner = self.buf[1..gt].to_string();
                            let len_before = self.buf.len();
                            self.buf.drain(..=gt);
                            self.process_tag(&tag_inner, &mut events);
                            // If process_tag put the tag back (incomplete element),
                            // the buffer grew back to its original size — stop draining.
                            if self.buf.len() >= len_before {
                                break;
                            }
                        }
                    }
                }
                Some(lt) => {
                    // Plain text before the next tag
                    let text: String = self.buf.drain(..lt).collect();
                    self.emit_text(&mut events, &text);
                    // Loop: buffer now starts at '<'
                }
            }
        }

        events
    }

    fn process_tag(&mut self, inner: &str, events: &mut Vec<XmlEvent>) {
        if let Some(rest) = inner.strip_prefix('/') {
            match rest {
                "component" => {
                    self.current_component = None;
                    self.in_bold = false;
                    self.current_inv_container = None;
                }
                "b" => {
                    self.in_bold = false;
                }
                "inv" => {
                    self.current_inv_container = None;
                }
                "dialogData" => {
                    self.current_dialog = None;
                }
                "compass" => {
                    if matches!(self.game, Game::DragonRealms) {
                        if self.dr_second_compass {
                            self.dr_second_compass = false;
                            events.push(XmlEvent::RoomCountBump);
                        } else {
                            self.dr_second_compass = true;
                        }
                    }
                }
                _ => {}
            }
            return;
        }
        if inner.starts_with('!') || inner.starts_with('?') {
            // Comment / processing instruction — skip
            return;
        }

        if inner.ends_with('/') {
            // Self-closing tag: <tag attr="val"/>
            let body = inner.trim_end_matches('/').trim_end();
            let tag_name_sc = body.split_whitespace().next().unwrap_or("");

            // Injury images inside <component id="injuries">
            if tag_name_sc == "image" && self.current_component.as_deref() == Some("injuries") {
                let xml = format!("<{}/>\n", body);
                let attrs = attrs_from_xml(&xml);
                if let Some(body_part) = attr(&attrs, "id") {
                    let name = attr(&attrs, "name").unwrap_or_default();
                    let (wound, scar) = parse_injury_name(&name);
                    events.push(XmlEvent::Injury { body_part, wound, scar });
                }
                return;
            }

            let xml = format!("<{}/>\n", body);
            if let Some(ev) = parse_empty_xml(&xml) {
                match ev {
                    XmlEvent::StylePush { ref id } => {
                        self.current_style = Some(id.clone());
                    }
                    XmlEvent::StylePop => {
                        self.current_style = None;
                    }
                    XmlEvent::Health { value, max } => {
                        tracing::info!("health={value}/{max:?}");
                        events.push(ev);
                    }
                    XmlEvent::Mana { value, max } => {
                        tracing::info!("mana={value}/{max:?}");
                        events.push(ev);
                    }
                    XmlEvent::PushStream { ref id } => {
                        if id == "familiar" {
                            self.fam_mode = FamMode::None;
                            self.fam_exits.clear();
                        }
                        if matches!(self.game, Game::DragonRealms) && id == "percWindow" {
                            self.dr_perc_tracking = true;
                            self.dr_perc_spells.clear();
                        }
                        self.current_stream = Some(id.clone());
                        events.push(ev);
                    }
                    XmlEvent::PopStream { ref id } => {
                        if id == "familiar" && !self.fam_exits.is_empty() {
                            let exits = std::mem::take(&mut self.fam_exits);
                            events.push(XmlEvent::FamiliarRoomExits { exits });
                            self.fam_mode = FamMode::None;
                        }
                        if id == "percWindow" && self.dr_perc_tracking {
                            events.push(XmlEvent::ClearActiveSpells);
                            for (name, duration) in std::mem::take(&mut self.dr_perc_spells) {
                                events.push(XmlEvent::ActiveSpell { name, duration });
                            }
                            self.dr_perc_tracking = false;
                        }
                        events.push(ev);
                        self.current_stream = None;
                    }
                    XmlEvent::ClearStream { .. } => {
                        events.push(ev);
                    }
                    _ => events.push(ev),
                }
            } else if let Some(ref dialog_id) = self.current_dialog {
                if tag_name_sc == "progressBar" {
                    let attrs = attrs_from_xml(&xml);
                    let text = attr(&attrs, "text").unwrap_or_default();
                    let time_str = attr(&attrs, "time").unwrap_or_default();
                    let secs = parse_time_to_secs(&time_str);
                    let id = attr(&attrs, "id").unwrap_or_default();
                    events.push(XmlEvent::DialogEntry {
                        dialog_id: dialog_id.clone(),
                        entry_id: id,
                        name: text,
                        duration_secs: secs,
                    });
                } else {
                    events.push(XmlEvent::Unknown { tag: tag_name_sc.to_string() });
                }
            } else {
                let tag = inner.split_whitespace().next().unwrap_or("").to_string();
                events.push(XmlEvent::Unknown { tag });
            }

            // DR: extract room ID and room name from streamWindow subtitle
            if matches!(self.game, Game::DragonRealms) && tag_name_sc == "streamWindow" {
                let attrs = attrs_from_xml(&xml);
                if attr(&attrs, "id").as_deref() == Some("main") {
                    if let Some(subtitle) = attr(&attrs, "subtitle") {
                        // Extract room title from brackets: [Room Name]
                        if let Some(start) = subtitle.find('[') {
                            if let Some(end) = subtitle.find(']') {
                                events.push(XmlEvent::RoomName { name: subtitle[start..=end].to_string() });
                            }
                        }
                        // Extract UID from parentheses: (12345)
                        if let Some(ps) = subtitle.rfind('(') {
                            if let Some(pe) = subtitle.rfind(')') {
                                if let Ok(uid) = subtitle[ps+1..pe].trim().parse::<u32>() {
                                    events.push(XmlEvent::RoomIdOnly { id: uid });
                                }
                            }
                        }
                    }
                }
            }
        } else {
            // Start tag: <tag attr="val">
            let tag_name = inner.split_whitespace().next().unwrap_or("");
            match tag_name {
                "settingsInfo" => {
                    let end_tag = "</settingsInfo>";
                    match self.buf.find(end_tag) {
                        None => {
                            self.buf.insert_str(0, &format!("<{}>", inner));
                        }
                        Some(offset) => {
                            self.buf.drain(..offset + end_tag.len());
                            let xml = format!("<{}/>", inner);
                            let attrs = attrs_from_xml(&xml);
                            if let Some(instance) = attr(&attrs, "instance") {
                                let game = Game::from_code(&instance);
                                self.game = game.clone();
                                events.push(XmlEvent::GameDetected { game });
                            }
                        }
                    }
                }
                "prompt" | "spell" => {
                    // Need to find the matching end tag before we can parse
                    let end_tag = format!("</{}>", tag_name);
                    match self.buf.find(&end_tag) {
                        None => {
                            // Not arrived yet — put the opening tag back and wait
                            self.buf.insert_str(0, &format!("<{}>", inner));
                        }
                        Some(offset) => {
                            let content = self.buf[..offset].to_string();
                            self.buf.drain(..offset + end_tag.len());
                            let plain = strip_tags(&content);
                            let xml = format!("<{}/>\n", inner); // parse attrs only
                            let attrs = attrs_from_xml(&xml);
                            if let Some(ev) = parse_start_tag(tag_name, &attrs, &plain) {
                                events.push(ev);
                            }
                        }
                    }
                }
                "component" => {
                    let xml = format!("<{}/>", inner);
                    let attrs = attrs_from_xml(&xml);
                    let id = attr(&attrs, "id").unwrap_or_default();
                    match id.as_str() {
                        // Text-content components: read to </component> and emit room events.
                        "room name" | "room desc" => {
                            let end_tag = "</component>";
                            match self.buf.find(end_tag) {
                                None => {
                                    self.buf.insert_str(0, &format!("<{}>", inner));
                                }
                                Some(offset) => {
                                    let content = self.buf[..offset].to_string();
                                    self.buf.drain(..offset + end_tag.len());
                                    let plain = strip_tags(&content);
                                    if let Some(ev) = parse_start_tag("component", &attrs, &plain) {
                                        events.push(ev);
                                    }
                                }
                            }
                        }
                        // Object-stream components: set context and emit ComponentClear.
                        "room objs" | "room players" | "inv" => {
                            events.push(XmlEvent::ComponentClear { component_id: id.clone() });
                            if id == "inv" { self.current_inv_container = None; }
                            self.current_component = Some(id);
                            self.in_bold = false;
                        }
                        _ => {
                            self.current_component = Some(id);
                            self.in_bold = false;
                        }
                    }
                }
                "dialogData" => {
                    let xml = format!("<{}/>", inner);
                    let attrs = attrs_from_xml(&xml);
                    let id = attr(&attrs, "id").unwrap_or_default();
                    if attr(&attrs, "clear").as_deref() == Some("t") {
                        events.push(XmlEvent::DialogClear { dialog_id: id.clone() });
                    }
                    self.current_dialog = Some(id);
                }
                "b" => {
                    self.in_bold = true;
                }
                "compass" if self.current_stream.as_deref() == Some("familiar") => {
                    self.fam_mode = FamMode::None;
                }
                "inv" => {
                    // <inv id="container_id"> scopes subsequent inventory items
                    let xml = format!("<{}/>", inner);
                    let attrs = attrs_from_xml(&xml);
                    self.current_inv_container = attr(&attrs, "id");
                }
                "a" => {
                    let end_tag = "</a>";
                    match self.buf.find(end_tag) {
                        None => {
                            self.buf.insert_str(0, &format!("<{}>", inner));
                        }
                        Some(offset) => {
                            let raw_content = self.buf[..offset].to_string();
                            self.buf.drain(..offset + end_tag.len());
                            let name = strip_tags(&raw_content);
                            if name.is_empty() { return; }
                            let xml = format!("<{}/>", inner);
                            let attrs = attrs_from_xml(&xml);
                            let id = attr(&attrs, "exist").unwrap_or_default();
                            let noun = attr(&attrs, "noun").unwrap_or_default();
                            if id.is_empty() { return; }
                            if self.current_stream.as_deref() == Some("familiar") {
                                let content_is_bold = raw_content.contains("<b>") || self.in_bold;
                                let category = match self.fam_mode {
                                    FamMode::Things if content_is_bold => ObjCategory::Npc,
                                    FamMode::Things => ObjCategory::Loot,
                                    FamMode::People => ObjCategory::Pc,
                                    FamMode::None => ObjCategory::RoomDesc,
                                    FamMode::Paths => {
                                        self.fam_exits.push(name);
                                        return;
                                    }
                                };
                                events.push(XmlEvent::FamiliarObjCreate {
                                    id, noun, name, category,
                                });
                                return;
                            }
                            let category = match self.current_component.as_deref() {
                                Some("room objs") if self.in_bold => ObjCategory::Npc,
                                Some("room objs") => ObjCategory::Loot,
                                Some("room players") => ObjCategory::Pc,
                                Some("inv") => ObjCategory::Inv {
                                    container: self.current_inv_container.clone(),
                                },
                                _ => return,
                            };
                            events.push(XmlEvent::GameObjCreate {
                                id, noun, name, category, status: None,
                            });
                        }
                    }
                }
                "right" | "left" => {
                    let hand = if tag_name == "right" { ObjHand::Right } else { ObjHand::Left };
                    let end_tag = format!("</{}>", tag_name);
                    match self.buf.find(&end_tag) {
                        None => {
                            self.buf.insert_str(0, &format!("<{}>", inner));
                        }
                        Some(offset) => {
                            let raw_content = self.buf[..offset].to_string();
                            self.buf.drain(..offset + end_tag.len());
                            let name = strip_tags(&raw_content);
                            let xml = format!("<{}/>", inner);
                            let attrs = attrs_from_xml(&xml);
                            let id = attr(&attrs, "exist").unwrap_or_default();
                            let noun = attr(&attrs, "noun").unwrap_or_default();
                            if id.is_empty() || name == "nothing" || name.trim().is_empty() {
                                events.push(XmlEvent::GameObjHandClear { hand });
                            } else {
                                events.push(XmlEvent::GameObjHandUpdate { hand, id, noun, name });
                            }
                        }
                    }
                }
                _ => {
                    // Container element — opening tag consumed, children handled naturally
                }
            }
        }
    }
}

impl Default for StreamParser {
    fn default() -> Self { Self::new(Game::default()) }
}

/// Convenience wrapper for tests: parse a static string as a single feed.
pub fn parse_chunk(input: &str) -> Vec<XmlEvent> {
    StreamParser::new(Game::default()).feed(input)
}

// ── Helpers ──────────────────────────────────────────────────────────────────

/// Find the `>` that closes the tag starting at `start` (which should be `<`),
/// correctly skipping over quoted attribute values.
fn find_gt(input: &str, start: usize) -> Option<usize> {
    let bytes = input.as_bytes();
    let mut in_quote: Option<u8> = None;
    let mut i = start + 1;
    while i < bytes.len() {
        match (in_quote, bytes[i]) {
            (None, b'"')  => in_quote = Some(b'"'),
            (None, b'\'') => in_quote = Some(b'\''),
            (Some(q), b) if b == q => in_quote = None,
            (None, b'>') => return Some(i),
            _ => {}
        }
        i += 1;
    }
    None
}

/// Strip XML tags from `s`, returning plain text (used for element content
/// that may contain inline markup like `<a>` link tags).
fn strip_tags(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut in_tag = false;
    for c in s.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => result.push(c),
            _ => {}
        }
    }
    quick_xml::escape::unescape(&result)
        .map(|c| c.into_owned())
        .unwrap_or(result)
}

/// Parse a single self-closing tag XML string and return the event, if any.
fn parse_empty_xml(xml: &str) -> Option<XmlEvent> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(false);
    if let Ok(Event::Empty(ref e)) = reader.read_event() {
        let tag = std::str::from_utf8(e.name().as_ref()).unwrap_or("").to_string();
        let attrs = collect_attrs(e);
        parse_empty_tag(&tag, &attrs)
    } else {
        None
    }
}

/// Extract attributes from a fake self-closing tag XML string.
fn attrs_from_xml(xml: &str) -> Attrs {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(false);
    if let Ok(Event::Empty(ref e)) = reader.read_event() {
        collect_attrs(e)
    } else {
        Vec::new()
    }
}

fn collect_attrs(e: &quick_xml::events::BytesStart<'_>) -> Attrs {
    e.attributes()
        .filter_map(|a| a.ok())
        .filter_map(|a| {
            let key = std::str::from_utf8(a.key.as_ref()).ok()?.to_string();
            let val = a.unescape_value().ok()?.into_owned();
            Some((key, val))
        })
        .collect()
}

impl StreamParser {
    fn emit_text(&mut self, events: &mut Vec<XmlEvent>, s: &str) {
        if s.is_empty() { return; }
        match self.current_stream.as_deref() {
            Some("familiar") => {
                match self.current_style.as_deref() {
                    Some("roomName") => {
                        self.fam_mode = FamMode::None;
                        self.fam_exits.clear();
                        events.push(XmlEvent::FamiliarRoomName { name: s.to_string() });
                    }
                    Some("roomDesc") => {
                        events.push(XmlEvent::FamiliarRoomDescription { text: s.to_string() });
                    }
                    _ => {
                        if s.contains("You also see") {
                            self.fam_mode = FamMode::Things;
                        } else if s.contains("Also here") {
                            self.fam_mode = FamMode::People;
                        } else if s.contains("Obvious paths") || s.contains("Obvious exits") {
                            self.fam_mode = FamMode::Paths;
                        }
                        events.push(XmlEvent::StreamText {
                            stream_id: "familiar".into(),
                            text: s.to_string(),
                        });
                    }
                }
            }
            Some(stream_id) => {
                if self.dr_perc_tracking && stream_id == "percWindow" {
                    if let Some(entry) = parse_dr_spell_line(s) {
                        self.dr_perc_spells.push(entry);
                    }
                } else {
                    events.push(XmlEvent::StreamText {
                        stream_id: stream_id.to_string(),
                        text: s.to_string(),
                    });
                }
            }
            None => {
                match self.current_style.as_deref() {
                    Some("roomName") => events.push(XmlEvent::RoomName { name: s.to_string() }),
                    Some("roomDesc") => events.push(XmlEvent::RoomDescription { text: s.to_string() }),
                    _ => events.push(XmlEvent::Text { content: s.to_string() }),
                }
            }
        }
    }
}

// ── Tag parsers ───────────────────────────────────────────────────────────────

type Attrs = Vec<(String, String)>;

fn attr(attrs: &Attrs, name: &str) -> Option<String> {
    attrs.iter().find(|(k, _)| k == name).map(|(_, v)| v.clone())
}

/// Parse "current/max" from the text attribute.
/// GemStone sends e.g. `text="health 150/200"` — the stat name prefix must be stripped.
/// Matches Lich5's `attributes['text'].scan(/-?\d+/)`: extract the first two integers.
fn parse_vital(attrs: &Attrs) -> (u32, Option<u32>) {
    let value: u32 = attr(attrs, "value").and_then(|v| v.parse().ok()).unwrap_or(0);
    if let Some(text) = attr(attrs, "text") {
        // Skip any non-numeric prefix (e.g. "health "), then split on '/'
        let numeric = text.trim_start_matches(|c: char| !c.is_ascii_digit());
        let parts: Vec<&str> = numeric.split('/').collect();
        if parts.len() >= 2 {
            let cur: u32 = parts[0].trim().parse().unwrap_or(value);
            let max: u32 = parts[1].trim().parse().unwrap_or(0);
            return (cur, Some(max));
        }
    }
    (value, None)
}

/// Parse a time string like "HH:MM:SS" into total seconds.
/// Returns `u32::MAX` for "Indefinite" or empty strings.
fn parse_time_to_secs(time: &str) -> u32 {
    if time == "Indefinite" || time.is_empty() { return u32::MAX; }
    let parts: Vec<&str> = time.split(':').collect();
    if parts.len() == 3 {
        let h: u32 = parts[0].parse().unwrap_or(0);
        let m: u32 = parts[1].parse().unwrap_or(0);
        let s: u32 = parts[2].parse().unwrap_or(0);
        h * 3600 + m * 60 + s
    } else {
        0
    }
}

fn parse_empty_tag(tag: &str, attrs: &Attrs) -> Option<XmlEvent> {
    match tag {
        "progressBar" => {
            let id = attr(attrs, "id").unwrap_or_default();
            match id.as_str() {
                "health"        => { let (v, m) = parse_vital(attrs); Some(XmlEvent::Health { value: v, max: m }) }
                "mana"          => { let (v, m) = parse_vital(attrs); Some(XmlEvent::Mana   { value: v, max: m }) }
                "spirit"        => { let (v, m) = parse_vital(attrs); Some(XmlEvent::Spirit { value: v, max: m }) }
                "stamina"       => { let (v, m) = parse_vital(attrs); Some(XmlEvent::Stamina { value: v, max: m }) }
                "concentration" => { let (v, m) = parse_vital(attrs); Some(XmlEvent::Concentration { value: v, max: m }) }
                "nextLvlPB" => {
                    attr(attrs, "value")
                        .and_then(|v| v.parse::<u64>().ok())
                        .map(|val| XmlEvent::Experience { value: val })
                }
                _ => None,
            }
        }
        "roundTime" => {
            let epoch: i64 = attr(attrs, "value")?.parse().ok()?;
            Some(XmlEvent::RoundTime { end_epoch: epoch })
        }
        "castTime" => {
            let epoch: i64 = attr(attrs, "value")?.parse().ok()?;
            Some(XmlEvent::CastTime { end_epoch: epoch })
        }
        "streamWindow" => {
            let id = attr(attrs, "id").unwrap_or_default();
            let title = attr(attrs, "title").unwrap_or_default();
            if id == "room exits" {
                Some(XmlEvent::RoomExits { exits: parse_exits(&title), raw: title })
            } else {
                Some(XmlEvent::StreamWindow { id, title })
            }
        }
        "indicator" => {
            let name = attr(attrs, "id").unwrap_or_default();
            let visible = attr(attrs, "visible").map(|v| v == "y").unwrap_or(false);
            Some(XmlEvent::Indicator { name, visible })
        }
        "mode" => {
            let id = attr(attrs, "id").unwrap_or_default();
            let room_id = attr(attrs, "roomId").and_then(|v| v.parse().ok());
            Some(XmlEvent::Mode { id, room_id })
        }
        "concentration" => { let (v, m) = parse_vital(attrs); Some(XmlEvent::Concentration { value: v, max: m }) }
        "rightHand" => {
            let item = attr(attrs, "noun").filter(|s| !s.is_empty());
            Some(XmlEvent::RightHand { item })
        }
        "leftHand" => {
            let item = attr(attrs, "noun").filter(|s| !s.is_empty());
            Some(XmlEvent::LeftHand { item })
        }
        "level" => {
            let value: u32 = attr(attrs, "value").and_then(|v| v.parse().ok()).unwrap_or(0);
            Some(XmlEvent::Level { value })
        }
        "nav" => {
            let id = attr(attrs, "rm").and_then(|v| v.parse().ok())?;
            Some(XmlEvent::RoomId { id })
        }
        "style" => {
            let id = attr(attrs, "id").unwrap_or_default();
            if id.is_empty() {
                Some(XmlEvent::StylePop)
            } else {
                Some(XmlEvent::StylePush { id })
            }
        }
        "pushStream" => {
            let id = attr(attrs, "id").unwrap_or_default();
            Some(XmlEvent::PushStream { id })
        }
        "popStream" => {
            let id = attr(attrs, "id").unwrap_or_default();
            Some(XmlEvent::PopStream { id })
        }
        "clearStream" => {
            let id = attr(attrs, "id").unwrap_or_default();
            Some(XmlEvent::ClearStream { id })
        }
        _ => None,
    }
}

fn parse_start_tag(tag: &str, attrs: &Attrs, text: &str) -> Option<XmlEvent> {
    match tag {
        "prompt" => {
            let time: i64 = attr(attrs, "time").and_then(|v| v.parse().ok()).unwrap_or(0);
            Some(XmlEvent::Prompt { time, text: text.to_string() })
        }
        "component" => {
            let id = attr(attrs, "id").unwrap_or_default();
            match id.as_str() {
                "room desc" => Some(XmlEvent::RoomDescription { text: text.to_string() }),
                "room name" => Some(XmlEvent::RoomName { name: text.to_string() }),
                _ => None,
            }
        }
        "spell" => {
            if attrs.iter().any(|(k, _)| k == "exist") {
                let duration: Option<u32> = attrs.iter()
                    .find(|(k, _)| k == "duration")
                    .and_then(|(_, v)| v.parse().ok());
                Some(XmlEvent::ActiveSpell { name: text.to_string(), duration })
            } else if text.is_empty() {
                Some(XmlEvent::SpellCleared)
            } else {
                Some(XmlEvent::PreparedSpell { name: text.to_string() })
            }
        }
        _ => None,
    }
}

fn parse_exits(title: &str) -> Vec<String> {
    title.strip_prefix("Obvious exits: ")
        .or_else(|| title.strip_prefix("Obvious exit: "))
        .or_else(|| title.strip_prefix("Obvious paths: "))
        .or_else(|| title.strip_prefix("Obvious path: "))
        .map(|s| s.split(", ").map(str::trim).map(String::from).collect())
        .unwrap_or_else(|| {
            if !title.is_empty() {
                tracing::warn!("parse_exits: unrecognized title format: {:?}", title);
            }
            Vec::new()
        })
}

/// Parse injury image name into (wound, scar) severity.
/// Examples: "Injury3" -> (3, 0), "Scar1" -> (0, 1), "Nsys2" -> (2, 0), "" -> (0, 0)
/// Parse a DR percWindow spell line into (name, duration).
/// Formats:
///   "Spell Name (HH:MM:SS)" → timed in seconds
///   "Spell Name (Indefinite)" → u32::MAX
///   "Spell Name (OM)" or "(fading)" → 0
///   "Spell Name (X%)" → 0 (stellar)
///   "Spell Name (N roisaen)" → N*15 seconds
///   "Spell Name (N anlaen)" → N*900 seconds
///   "Spell Name" (no parens) → u32::MAX (indefinite)
fn parse_dr_spell_line(line: &str) -> Option<(String, Option<u32>)> {
    let trimmed = line.trim();
    if trimmed.is_empty() { return None; }

    if let Some(paren_start) = trimmed.rfind('(') {
        if let Some(paren_end) = trimmed.rfind(')') {
            let name = trimmed[..paren_start].trim().to_string();
            if name.is_empty() { return None; }
            let inside = trimmed[paren_start+1..paren_end].trim();
            let duration = if inside.eq_ignore_ascii_case("indefinite") {
                Some(u32::MAX)
            } else if inside.eq_ignore_ascii_case("fading") || inside == "OM" {
                Some(0)
            } else if inside.ends_with('%') {
                Some(0)
            } else if inside.contains("roisaen") {
                let n: u32 = inside.split_whitespace().next()
                    .and_then(|s| s.parse().ok()).unwrap_or(0);
                Some(n * 15)
            } else if inside.contains("anlaen") {
                let n: u32 = inside.split_whitespace().next()
                    .and_then(|s| s.parse().ok()).unwrap_or(0);
                Some(n * 900)
            } else {
                // Try HH:MM:SS
                let parts: Vec<&str> = inside.split(':').collect();
                if parts.len() == 3 {
                    let h: u32 = parts[0].parse().unwrap_or(0);
                    let m: u32 = parts[1].parse().unwrap_or(0);
                    let s: u32 = parts[2].parse().unwrap_or(0);
                    Some(h * 3600 + m * 60 + s)
                } else {
                    Some(0)
                }
            };
            return Some((name, duration));
        }
    }

    // No parentheses — indefinite
    Some((trimmed.to_string(), Some(u32::MAX)))
}

fn parse_injury_name(name: &str) -> (u8, u8) {
    let digit = name.chars().find(|c| c.is_ascii_digit())
        .and_then(|c| c.to_digit(10))
        .map(|d| d.min(3))
        .unwrap_or(0) as u8;
    if digit == 0 {
        return (0, 0);
    }
    if name.starts_with("Injury") || name.starts_with("injury") {
        (digit, 0)
    } else if name.starts_with("Scar") || name.starts_with("scar") {
        (0, digit)
    } else if name.starts_with("Nsys") || name.starts_with("nsys") {
        (digit, 0)
    } else {
        tracing::warn!("parse_injury_name: unrecognized injury name prefix: {name:?}");
        (0, 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_push_pop_stream_bounty() {
        let events = parse_chunk(
            r#"<pushStream id="bounty"/>You have a task<popStream id="bounty"/>"#
        );
        assert!(matches!(&events[0], XmlEvent::PushStream { id } if id == "bounty"));
        assert!(matches!(&events[1], XmlEvent::StreamText { stream_id, text }
            if stream_id == "bounty" && text == "You have a task"));
        assert!(matches!(&events[2], XmlEvent::PopStream { id } if id == "bounty"));
    }

    #[test]
    fn test_clear_stream() {
        let events = parse_chunk(r#"<clearStream id="bounty"/>"#);
        assert!(matches!(&events[0], XmlEvent::ClearStream { id } if id == "bounty"));
    }

    #[test]
    fn test_stream_text_not_room_name() {
        let mut parser = StreamParser::default();
        let events = parser.feed(
            r#"<pushStream id="familiar"/><style id="roomName"/>Some Room<style id=""/><popStream id="familiar"/>"#
        );
        assert!(!events.iter().any(|e| matches!(e, XmlEvent::RoomName { .. })));
        assert!(events.iter().any(|e| matches!(e, XmlEvent::FamiliarRoomName { .. })));
    }

    #[test]
    fn test_split_push_stream_across_feeds() {
        let mut parser = StreamParser::default();
        let e1 = parser.feed(r#"<pushStream id="bou"#);
        assert!(e1.is_empty());
        let e2 = parser.feed(r#"nty"/>task text<popStream id="bounty"/>"#);
        assert!(matches!(&e2[0], XmlEvent::PushStream { id } if id == "bounty"));
        assert!(matches!(&e2[1], XmlEvent::StreamText { stream_id, .. } if stream_id == "bounty"));
    }

    #[test]
    fn test_familiar_stream_room_name() {
        let events = parse_chunk(
            r#"<pushStream id="familiar"/><style id="roomName"/>[Icemule Trace]<style id=""/><popStream id="familiar"/>"#
        );
        assert!(events.iter().any(|e| matches!(e, XmlEvent::FamiliarRoomName { name } if name == "[Icemule Trace]")));
        assert!(!events.iter().any(|e| matches!(e, XmlEvent::RoomName { .. })));
    }

    #[test]
    fn test_familiar_stream_npc_loot() {
        let mut parser = StreamParser::default();
        let events = parser.feed(concat!(
            r#"<pushStream id="familiar"/>"#,
            "You also see ",
            r#"<a exist="123" noun="kobold"><b>a kobold</b></a>"#,
            " and ",
            r#"<a exist="456" noun="chest">a wooden chest</a>"#,
            r#"<popStream id="familiar"/>"#
        ));
        let fam_objs: Vec<_> = events.iter()
            .filter(|e| matches!(e, XmlEvent::FamiliarObjCreate { .. }))
            .collect();
        assert_eq!(fam_objs.len(), 2);
        assert!(matches!(&fam_objs[0], XmlEvent::FamiliarObjCreate { category: ObjCategory::Npc, .. }));
        assert!(matches!(&fam_objs[1], XmlEvent::FamiliarObjCreate { category: ObjCategory::Loot, .. }));
    }

    #[test]
    fn test_familiar_stream_pcs() {
        let mut parser = StreamParser::default();
        let events = parser.feed(concat!(
            r#"<pushStream id="familiar"/>"#,
            "Also here: ",
            r#"<a exist="789" noun="Gandalf">Gandalf</a>"#,
            r#"<popStream id="familiar"/>"#
        ));
        assert!(events.iter().any(|e| matches!(e,
            XmlEvent::FamiliarObjCreate { category: ObjCategory::Pc, noun, .. } if noun == "Gandalf"
        )));
    }

    #[test]
    fn test_familiar_room_desc_objects() {
        let mut parser = StreamParser::default();
        let events = parser.feed(concat!(
            r#"<pushStream id="familiar"/>"#,
            r#"<style id="roomDesc"/>The trail is narrow. "#,
            r#"<a exist="111" noun="sign">a wooden sign</a>"#,
            r#"<style id=""/>"#,
            r#"<popStream id="familiar"/>"#
        ));
        assert!(events.iter().any(|e| matches!(e,
            XmlEvent::FamiliarObjCreate { category: ObjCategory::RoomDesc, .. }
        )));
    }

    #[test]
    fn test_normal_parsing_unaffected_by_stream_code() {
        let mut parser = StreamParser::default();
        let events = parser.feed(
            r#"<style id="roomName"/>Town Square<style id=""/>"#
        );
        assert!(events.iter().any(|e| matches!(e, XmlEvent::RoomName { name } if name == "Town Square")));
        assert!(!events.iter().any(|e| matches!(e, XmlEvent::FamiliarRoomName { .. })));
    }
}
