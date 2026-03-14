use revenant::xml_parser::{StreamParser, XmlEvent, ObjCategory};
use revenant::game_state::GameState;
use revenant::game_obj::GameObjRegistry;

#[test]
fn test_full_familiar_stream_pipeline() {
    let mut parser = StreamParser::new();
    let mut gs = GameState::default();
    let mut registry = GameObjRegistry::new();

    let xml = concat!(
        r#"<pushStream id="familiar"/>"#,
        r#"<style id="roomName"/>[Icemule Trace, East]<style id=""/>"#,
        r#"<style id="roomDesc"/>The narrow trail winds through snow.<style id=""/>"#,
        "\n",
        "You also see ",
        r#"<a exist="12345" noun="kobold"><b>a kobold</b></a>"#,
        " and ",
        r#"<a exist="12346" noun="chest">a wooden chest</a>"#,
        ".\n",
        "Also here: ",
        r#"<a exist="12347" noun="Gandalf">Gandalf</a>"#,
        ".\n",
        r#"<popStream id="familiar"/>"#,
    );

    let events = parser.feed(xml);

    for event in &events {
        match event {
            XmlEvent::FamiliarRoomName { .. }
            | XmlEvent::FamiliarRoomDescription { .. }
            | XmlEvent::FamiliarRoomExits { .. }
            | XmlEvent::StreamText { .. }
            | XmlEvent::ClearStream { .. }
            | XmlEvent::PopStream { .. } => {
                gs.apply(event.clone());
            }
            XmlEvent::FamiliarObjCreate { id, noun, name, category } => {
                match category {
                    ObjCategory::Npc      => registry.new_fam_npc(id, noun, name),
                    ObjCategory::Loot     => registry.new_fam_loot(id, noun, name),
                    ObjCategory::Pc       => registry.new_fam_pc(id, noun, name),
                    ObjCategory::RoomDesc => registry.new_fam_room_desc(id, noun, name),
                    _ => {}
                }
            }
            _ => {}
        }
    }

    assert_eq!(gs.familiar_room_title, "[Icemule Trace, East]");
    assert!(gs.familiar_room_description.contains("narrow trail"));
    assert_eq!(registry.fam_npcs.len(), 1);
    assert_eq!(registry.fam_npcs[0].noun, "kobold");
    assert_eq!(registry.fam_loot.len(), 1);
    assert_eq!(registry.fam_loot[0].noun, "chest");
    assert_eq!(registry.fam_pcs.len(), 1);
    assert_eq!(registry.fam_pcs[0].noun, "Gandalf");
}

#[test]
fn test_bounty_society_pipeline() {
    let mut parser = StreamParser::new();
    let mut gs = GameState::default();

    let xml = concat!(
        r#"<clearStream id="bounty"/>"#,
        r#"<pushStream id="bounty"/>  You have been tasked to hunt down and kill 3 troll kings  <popStream id="bounty"/>"#,
        r#"<pushStream id="society"/>You are a Master in the Order of Voln.<popStream id="society"/>"#,
    );

    let events = parser.feed(xml);
    for event in events {
        gs.apply(event);
    }

    assert_eq!(gs.bounty_task, "You have been tasked to hunt down and kill 3 troll kings");
    assert_eq!(gs.society_task, "You are a Master in the Order of Voln.");
}

#[test]
fn test_normal_parsing_unaffected_by_stream_code() {
    let mut parser = StreamParser::new();
    let events = parser.feed(
        r#"<style id="roomName"/>Town Square<style id=""/>"#
    );
    assert!(events.iter().any(|e| matches!(e, XmlEvent::RoomName { name } if name == "Town Square")));
    assert!(!events.iter().any(|e| matches!(e, XmlEvent::FamiliarRoomName { .. })));
}

#[test]
fn test_familiar_stream_split_feeds() {
    let mut parser = StreamParser::new();
    let e1 = parser.feed(r#"<pushStream id="familiar"/><style id="roomName"/>Room"#);
    let e2 = parser.feed(r#" Name<style id=""/><popStream id="familiar"/>"#);
    let all: Vec<_> = e1.into_iter().chain(e2).collect();
    let fam_names: Vec<_> = all.iter()
        .filter(|e| matches!(e, XmlEvent::FamiliarRoomName { .. }))
        .collect();
    assert!(!fam_names.is_empty());
}
