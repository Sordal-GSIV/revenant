use revenant::xml_parser::{XmlEvent, parse_chunk};

#[test]
fn test_parse_health_tag() {
    let xml = r#"<health id="health" value="150" text="150"/>"#;
    let events = parse_chunk(xml);
    assert!(events.iter().any(|e| matches!(e, XmlEvent::Health { value: 150, .. })));
}

#[test]
fn test_parse_max_health_from_text_attribute() {
    let xml = r#"<health id="health" value="150" text="150/200"/>"#;
    let events = parse_chunk(xml);
    assert!(events.iter().any(|e| matches!(e, XmlEvent::Health { value: 150, max: Some(200) })));
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
    assert!(events.iter().any(|e| matches!(e, XmlEvent::Prompt { .. })));
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
