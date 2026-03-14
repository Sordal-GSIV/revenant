use revenant::xml_parser::{StreamParser, XmlEvent};

#[test]
fn test_injury_wound_head() {
    let mut parser = StreamParser::new();
    let events = parser.feed(r#"<component id="injuries"><image id="head" name="Injury3"/></component>"#);
    let injury = events.iter().find(|e| matches!(e, XmlEvent::Injury { .. }));
    assert!(injury.is_some(), "expected Injury event, got: {events:?}");
    match injury.unwrap() {
        XmlEvent::Injury { body_part, wound, scar } => {
            assert_eq!(body_part, "head");
            assert_eq!(*wound, 3);
            assert_eq!(*scar, 0);
        }
        _ => unreachable!(),
    }
}

#[test]
fn test_injury_scar_chest() {
    let mut parser = StreamParser::new();
    let events = parser.feed(r#"<component id="injuries"><image id="chest" name="Scar1"/></component>"#);
    let injury = events.iter().find(|e| matches!(e, XmlEvent::Injury { .. }));
    assert!(injury.is_some(), "expected Injury event");
    match injury.unwrap() {
        XmlEvent::Injury { body_part, wound, scar } => {
            assert_eq!(body_part, "chest");
            assert_eq!(*wound, 0);
            assert_eq!(*scar, 1);
        }
        _ => unreachable!(),
    }
}

#[test]
fn test_injury_nsys_treated_as_wound() {
    let mut parser = StreamParser::new();
    let events = parser.feed(r#"<component id="injuries"><image id="nsys" name="Nsys2"/></component>"#);
    let injury = events.iter().find(|e| matches!(e, XmlEvent::Injury { .. }));
    assert!(injury.is_some());
    match injury.unwrap() {
        XmlEvent::Injury { body_part, wound, scar } => {
            assert_eq!(body_part, "nsys");
            assert_eq!(*wound, 2);
            assert_eq!(*scar, 0);
        }
        _ => unreachable!(),
    }
}

#[test]
fn test_injury_empty_name_clears() {
    let mut parser = StreamParser::new();
    let events = parser.feed(r#"<component id="injuries"><image id="leftArm" name=""/></component>"#);
    let injury = events.iter().find(|e| matches!(e, XmlEvent::Injury { .. }));
    assert!(injury.is_some());
    match injury.unwrap() {
        XmlEvent::Injury { body_part, wound, scar } => {
            assert_eq!(body_part, "leftArm");
            assert_eq!(*wound, 0);
            assert_eq!(*scar, 0);
        }
        _ => unreachable!(),
    }
}

#[test]
fn test_injury_multiple_in_one_component() {
    let mut parser = StreamParser::new();
    let events = parser.feed(
        r#"<component id="injuries"><image id="head" name="Injury3"/><image id="chest" name="Scar1"/><image id="leftArm" name=""/></component>"#,
    );
    let injuries: Vec<_> = events.iter().filter(|e| matches!(e, XmlEvent::Injury { .. })).collect();
    assert_eq!(injuries.len(), 3, "expected 3 injury events, got: {injuries:?}");
}

#[test]
fn test_image_outside_injuries_not_parsed_as_injury() {
    let mut parser = StreamParser::new();
    // Image inside a different component should NOT produce Injury events
    let events = parser.feed(r#"<component id="room objs"><image id="head" name="Injury3"/></component>"#);
    let injuries: Vec<_> = events.iter().filter(|e| matches!(e, XmlEvent::Injury { .. })).collect();
    assert_eq!(injuries.len(), 0, "image outside injuries component should not emit Injury");
}
