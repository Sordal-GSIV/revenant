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

/// Parse a chunk of the GemStone XML stream into events.
/// The stream mixes XML tags with plain text. This parser is line-oriented
/// and best-effort — partial tags across TCP packet boundaries will appear
/// as Text events and be forwarded unchanged to the client.
///
/// Tracks `<style id="roomName"/>` / `<style id="roomDesc"/>` within the
/// chunk to emit RoomName / RoomDescription events from styled text.
pub fn parse_chunk(input: &str) -> Vec<XmlEvent> {
    let mut events = Vec::new();
    let mut reader = Reader::from_str(input);
    reader.config_mut().trim_text(false);

    let mut current_style: Option<String> = None;

    loop {
        match reader.read_event() {
            Ok(Event::Empty(ref e)) => {
                let tag = std::str::from_utf8(e.name().as_ref()).unwrap_or("").to_string();
                let attrs: Vec<(String, String)> = e
                    .attributes()
                    .filter_map(|a| a.ok())
                    .filter_map(|a| {
                        let key = std::str::from_utf8(a.key.as_ref()).ok()?.to_string();
                        let val = a.unescape_value().ok()?.into_owned();
                        Some((key, val))
                    })
                    .collect();
                if let Some(ev) = parse_empty_tag(&tag, &attrs) {
                    match &ev {
                        XmlEvent::StylePush { id } => { current_style = Some(id.clone()); }
                        XmlEvent::StylePop => { current_style = None; }
                        _ => { events.push(ev); }
                    }
                } else {
                    events.push(XmlEvent::Unknown { tag });
                }
            }
            Ok(Event::Start(ref e)) => {
                let tag = std::str::from_utf8(e.name().as_ref()).unwrap_or("").to_string();
                let attrs: Vec<(String, String)> = e
                    .attributes()
                    .filter_map(|a| a.ok())
                    .filter_map(|a| {
                        let key = std::str::from_utf8(a.key.as_ref()).ok()?.to_string();
                        let val = a.unescape_value().ok()?.into_owned();
                        Some((key, val))
                    })
                    .collect();
                let name = e.name().clone();
                let raw = reader.read_text(name).unwrap_or_default();
                let text = quick_xml::escape::unescape(raw.as_ref())
                    .unwrap_or_else(|_| raw.clone())
                    .into_owned();
                if let Some(ev) = parse_start_tag(&tag, &attrs, &text) {
                    events.push(ev);
                }
            }
            Ok(Event::Text(ref t)) => {
                let s = t.decode().unwrap_or_default().into_owned();
                if !s.is_empty() {
                    // Styled text: <style id="roomName"/> text <style id=""/>
                    match current_style.as_deref() {
                        Some("roomName") => {
                            events.push(XmlEvent::RoomName { name: s });
                        }
                        Some("roomDesc") => {
                            events.push(XmlEvent::RoomDescription { text: s });
                        }
                        _ => {
                            events.push(XmlEvent::Text { content: s });
                        }
                    }
                }
            }
            Ok(Event::GeneralRef(ref e)) => {
                let name = std::str::from_utf8(e.as_ref()).unwrap_or("");
                let raw = format!("&{};", name);
                events.push(XmlEvent::Text { content: raw });
            }
            Ok(Event::Eof) => break,
            Err(e) => {
                tracing::debug!("XML parse error (partial chunk): {}", e);
                break;
            }
            _ => {}
        }
    }
    events
}

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
        // GemStone sends vitals as <progressBar id="health" text="100/200" value="100"/>
        // (not as <health .../> — that tag never appears in the live stream)
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
            // room ID comes from <nav rm="123"/>
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
