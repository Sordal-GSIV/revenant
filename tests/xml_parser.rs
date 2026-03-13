use revenant::xml_parser::{XmlEvent, parse_chunk};

#[test]
fn test_parse_health_tag() {
    // GemStone sends vitals as <progressBar id="health" .../>
    let xml = r#"<progressBar id="health" value="150" text="150"/>"#;
    let events = parse_chunk(xml);
    assert!(events.iter().any(|e| matches!(e, XmlEvent::Health { value: 150, .. })));
}

#[test]
fn test_parse_max_health_from_text_attribute() {
    // GemStone includes the stat name prefix: "health 150/200"
    let xml = r#"<progressBar id="health" value="150" text="health 150/200"/>"#;
    let events = parse_chunk(xml);
    assert!(events.iter().any(|e| matches!(e, XmlEvent::Health { value: 150, max: Some(200) })));
}

#[test]
fn test_parse_vitals_inside_dialogdata() {
    // GemStone wraps progressBar tags inside <dialogData> — must not be swallowed
    let xml = r#"<dialogData id='minivitals'><progressBar id="health" value="75" text="health 150/200"/><progressBar id="mana" value="60" text="mana 120/200"/></dialogData>"#;
    let events = parse_chunk(xml);
    assert!(events.iter().any(|e| matches!(e, XmlEvent::Health { value: 150, max: Some(200) })),
        "Health not found in: {events:?}");
    assert!(events.iter().any(|e| matches!(e, XmlEvent::Mana { value: 120, max: Some(200) })),
        "Mana not found in: {events:?}");
}

#[test]
fn test_parse_style_room_name() {
    let xml = "<style id=\"roomName\"/>The Cobblestone Street<style id=\"\"/>";
    let events = parse_chunk(xml);
    assert!(events.iter().any(|e| matches!(e, XmlEvent::RoomName { name } if name.contains("Cobblestone"))));
}

#[test]
fn test_parse_roundtime() {
    let xml = r#"<roundTime value="9999999999"/>"#;
    let events = parse_chunk(xml);
    assert!(events.iter().any(|e| matches!(e, XmlEvent::RoundTime { .. })));
}

#[test]
fn test_parse_prompt() {
    let xml = r#"<prompt time="1234567890">&gt;</prompt>"#;
    let events = parse_chunk(xml);
    assert!(events.iter().any(|e| matches!(e, XmlEvent::Prompt { text, .. } if text == ">")));
}

#[test]
fn test_parse_room_exits() {
    let xml = r#"<streamWindow id="room exits" title="Obvious exits: north, east"/>"#;
    let events = parse_chunk(xml);
    match events.iter().find(|e| matches!(e, XmlEvent::RoomExits { .. })) {
        Some(XmlEvent::RoomExits { exits }) => {
            assert_eq!(exits, &vec!["north".to_string(), "east".to_string()]);
        }
        _ => panic!("Expected RoomExits"),
    }
}

#[test]
fn test_plain_text_passthrough() {
    let text = "You see a narrow cobblestone street.\n";
    let events = parse_chunk(text);
    assert!(events.iter().any(|e| matches!(e, XmlEvent::Text { content } if content.contains("cobblestone"))));
}
