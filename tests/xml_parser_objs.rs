use revenant::xml_parser::{parse_chunk, XmlEvent, ObjCategory, ObjHand};

#[test]
fn test_loot_in_room_objs_component() {
    let xml = "<component id='room objs'><a exist=\"-456\" noun=\"sword\">a rusty sword</a></component>";
    let events = parse_chunk(xml);
    assert!(
        events.iter().any(|e| matches!(e,
            XmlEvent::GameObjCreate { id, noun, name, category, .. }
            if id == "-456" && noun == "sword" && name == "a rusty sword"
               && *category == ObjCategory::Loot
        )),
        "Expected loot event, got: {events:?}"
    );
}

#[test]
fn test_npc_bold_in_room_objs() {
    // NPC names are wrapped in <b> tags
    let xml = "<component id='room objs'><b><a exist=\"-123\" noun=\"goblin\">a snarling goblin</a></b></component>";
    let events = parse_chunk(xml);
    assert!(
        events.iter().any(|e| matches!(e,
            XmlEvent::GameObjCreate { id, category, .. }
            if id == "-123" && *category == ObjCategory::Npc
        )),
        "Expected NPC event, got: {events:?}"
    );
}

#[test]
fn test_pc_in_room_players_component() {
    let xml = "<component id='room players'><a exist=\"99999\" noun=\"Sordal\">Sordal Goldenleaf</a></component>";
    let events = parse_chunk(xml);
    assert!(
        events.iter().any(|e| matches!(e,
            XmlEvent::GameObjCreate { id, category, .. }
            if id == "99999" && *category == ObjCategory::Pc
        )),
        "Expected PC event, got: {events:?}"
    );
}

#[test]
fn test_room_desc_component_emits_text_not_obj() {
    // <component id='room desc'> carries room description TEXT, not <a> objects.
    // It should emit XmlEvent::RoomDescription (existing behaviour), not GameObjCreate.
    let xml = "<component id='room desc'>A narrow cobblestone street.</component>";
    let events = parse_chunk(xml);
    assert!(
        events.iter().any(|e| matches!(e, XmlEvent::RoomDescription { .. })),
        "Expected RoomDescription, got: {events:?}"
    );
    assert!(
        !events.iter().any(|e| matches!(e, XmlEvent::GameObjCreate { .. })),
        "Should not emit GameObjCreate for room desc text"
    );
}

#[test]
fn test_right_hand_with_exist() {
    let xml = "<right exist=\"-456\" noun=\"sword\">a steel sword</right>";
    let events = parse_chunk(xml);
    assert!(
        events.iter().any(|e| matches!(e,
            XmlEvent::GameObjHandUpdate { hand: ObjHand::Right, id, noun, .. }
            if id == "-456" && noun == "sword"
        )),
        "Expected right hand update, got: {events:?}"
    );
}

#[test]
fn test_left_hand_nothing_clears() {
    let xml = "<left exist=\"\" noun=\"\">nothing</left>";
    let events = parse_chunk(xml);
    assert!(
        events.iter().any(|e| matches!(e, XmlEvent::GameObjHandClear { hand: ObjHand::Left })),
        "Expected left hand clear, got: {events:?}"
    );
}

#[test]
fn test_component_clear_emitted_for_room_objs() {
    let xml = "<component id='room objs'></component>";
    let events = parse_chunk(xml);
    assert!(
        events.iter().any(|e| matches!(e,
            XmlEvent::ComponentClear { component_id }
            if component_id == "room objs"
        )),
        "Expected ComponentClear for room objs, got: {events:?}"
    );
}

#[test]
fn test_obj_tag_split_across_chunks() {
    // Simulate TCP split mid-tag
    let mut parser = revenant::xml_parser::StreamParser::new();
    let chunk1 = "<component id='room objs'><a exist=\"-123\" noun=\"goblin\">a ";
    let chunk2 = "snarling goblin</a></component>";
    let e1 = parser.feed(chunk1);
    assert!(e1.iter().all(|e| !matches!(e, XmlEvent::GameObjCreate { .. })),
        "Should not emit before tag is complete");
    let e2 = parser.feed(chunk2);
    assert!(
        e2.iter().any(|e| matches!(e,
            XmlEvent::GameObjCreate { id, name, .. }
            if id == "-123" && name == "a snarling goblin"
        )),
        "Expected object after second chunk, got: {e2:?}"
    );
}

#[test]
fn test_existing_events_unaffected() {
    // Regression: existing health parsing still works after parser changes
    let xml = r#"<progressBar id="health" value="150" text="health 150/200"/>"#;
    let events = parse_chunk(xml);
    assert!(events.iter().any(|e| matches!(e, XmlEvent::Health { value: 150, .. })));
}
