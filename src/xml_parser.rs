use quick_xml::events::Event;
use quick_xml::Reader;

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
    RoomExits { exits: Vec<String> },
    RoomId { id: u32 },
    PreparedSpell { name: String },
    SpellCleared,
    ActiveSpell { name: String, duration: Option<u32> },
    Indicator { name: String, visible: bool },
    RightHand { item: Option<String> },
    LeftHand { item: Option<String> },
    Level { value: u32 },
    Text { content: String },
    StreamWindow { id: String, title: String },
    Mode { id: String, room_id: Option<u32> },
    /// <style id="roomName"/> … <style id=""/> styled-text room name/desc
    StylePush { id: String },
    StylePop,
    Unknown { tag: String },
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
}

impl StreamParser {
    pub fn new() -> Self {
        Self { buf: String::new(), current_style: None }
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
                    emit_text(&mut events, &text, &self.current_style);
                    break;
                }
                Some(0) => {
                    // Buffer starts with a tag; find its closing '>'
                    match find_gt(&self.buf, 0) {
                        None => break, // Tag incomplete — wait for more data
                        Some(gt) => {
                            let tag_inner = self.buf[1..gt].to_string();
                            self.buf.drain(..=gt);
                            self.process_tag(&tag_inner, &mut events);
                        }
                    }
                }
                Some(lt) => {
                    // Plain text before the next tag
                    let text: String = self.buf.drain(..lt).collect();
                    emit_text(&mut events, &text, &self.current_style);
                    // Loop: buffer now starts at '<'
                }
            }
        }

        events
    }

    fn process_tag(&mut self, inner: &str, events: &mut Vec<XmlEvent>) {
        if inner.starts_with('/') {
            // End tag — ignored (nesting not tracked)
            return;
        }
        if inner.starts_with('!') || inner.starts_with('?') {
            // Comment / processing instruction — skip
            return;
        }

        if inner.ends_with('/') {
            // Self-closing tag: <tag attr="val"/>
            let body = inner.trim_end_matches('/').trim_end();
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
                    _ => events.push(ev),
                }
            } else {
                let tag = inner.split_whitespace().next().unwrap_or("").to_string();
                events.push(XmlEvent::Unknown { tag });
            }
        } else {
            // Start tag: <tag attr="val">
            let tag_name = inner.split_whitespace().next().unwrap_or("");
            match tag_name {
                "prompt" | "component" | "spell" => {
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
                _ => {
                    // Container element — opening tag consumed, children handled naturally
                }
            }
        }
    }
}

impl Default for StreamParser {
    fn default() -> Self { Self::new() }
}

/// Convenience wrapper for tests: parse a static string as a single feed.
pub fn parse_chunk(input: &str) -> Vec<XmlEvent> {
    StreamParser::new().feed(input)
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

fn emit_text(events: &mut Vec<XmlEvent>, s: &str, style: &Option<String>) {
    if s.is_empty() { return; }
    match style.as_deref() {
        Some("roomName") => events.push(XmlEvent::RoomName { name: s.to_string() }),
        Some("roomDesc") => events.push(XmlEvent::RoomDescription { text: s.to_string() }),
        _ => events.push(XmlEvent::Text { content: s.to_string() }),
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
                Some(XmlEvent::RoomExits { exits: parse_exits(&title) })
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
        .map(|s| s.split(", ").map(str::trim).map(String::from).collect())
        .unwrap_or_else(|| {
            if !title.is_empty() {
                tracing::warn!("parse_exits: unrecognized title format: {:?}", title);
            }
            Vec::new()
        })
}
