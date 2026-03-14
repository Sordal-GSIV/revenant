use revenant::game_state::{GameState, Stance, MindState, EncumbranceState, Game};
use revenant::xml_parser::XmlEvent;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

#[test]
fn test_default_game_state_is_zero() {
    let gs = GameState::default();
    assert_eq!(gs.health, 0);
    assert_eq!(gs.max_health, 0);
    assert_eq!(gs.mana, 0);
    assert!(gs.room_name.is_empty());
    assert!(gs.room_exits.is_empty());
    assert!(!gs.bleeding);
    assert_eq!(gs.roundtime(), 0.0);
}

#[test]
fn test_roundtime_expired_clamps_to_zero() {
    let mut gs = GameState::default();
    gs.roundtime_end = Some(Instant::now() - Duration::from_secs(5));
    assert_eq!(gs.roundtime(), 0.0);
}

#[test]
fn test_roundtime_future_is_positive() {
    let mut gs = GameState::default();
    gs.roundtime_end = Some(Instant::now() + Duration::from_secs(10));
    let rt = gs.roundtime();
    assert!(rt > 0.0 && rt <= 10.0);
}

#[test]
fn test_apply_health_and_max() {
    let mut gs = GameState::default();
    gs.apply(XmlEvent::Health { value: 123, max: Some(200) });
    assert_eq!(gs.health, 123);
    assert_eq!(gs.max_health, 200);
}

#[test]
fn test_apply_room_exits() {
    let mut gs = GameState::default();
    gs.apply(XmlEvent::RoomExits { exits: vec!["north".into(), "east".into()] });
    assert_eq!(gs.room_exits, vec!["north", "east"]);
}

#[test]
fn test_apply_roundtime_future() {
    let mut gs = GameState::default();
    let future = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() as i64 + 5;
    gs.apply(XmlEvent::RoundTime { end_epoch: future });
    let rt = gs.roundtime();
    assert!(rt > 0.0 && rt <= 5.1, "roundtime was {rt}");
}

#[test]
fn test_apply_roundtime_past_gives_zero() {
    let mut gs = GameState::default();
    gs.apply(XmlEvent::RoundTime { end_epoch: 1 }); // epoch 1 is in the past
    assert_eq!(gs.roundtime(), 0.0);
}

#[test]
fn test_apply_indicator_bleeding() {
    let mut gs = GameState::default();
    gs.apply(XmlEvent::Indicator { name: "IconBLEEDING".into(), visible: true });
    assert!(gs.bleeding);
    gs.apply(XmlEvent::Indicator { name: "IconBLEEDING".into(), visible: false });
    assert!(!gs.bleeding);
}

#[test]
fn test_apply_prepared_spell_and_clear() {
    let mut gs = GameState::default();
    gs.apply(XmlEvent::PreparedSpell { name: "Spirit Shield".into() });
    assert_eq!(gs.prepared_spell, Some("Spirit Shield".to_string()));
    gs.apply(XmlEvent::SpellCleared);
    assert!(gs.prepared_spell.is_none());
}

#[test]
fn test_apply_mode_sets_room_id() {
    let mut gs = GameState::default();
    gs.apply(XmlEvent::Mode { id: "GAME".into(), room_id: Some(42) });
    assert_eq!(gs.room_id, Some(42));
    // Mode with no room_id should not clear the existing room_id
    gs.apply(XmlEvent::Mode { id: "GAME".into(), room_id: None });
    assert_eq!(gs.room_id, Some(42));
}

#[test]
fn test_stance_to_str_and_value() {
    assert_eq!(Stance::None.as_str(), None);
    assert_eq!(Stance::None.to_value(), None);
    assert_eq!(Stance::Offensive.as_str(), Some("offensive"));
    assert_eq!(Stance::Offensive.to_value(), Some(100));
    assert_eq!(Stance::Defensive.as_str(), Some("defensive"));
    assert_eq!(Stance::Defensive.to_value(), Some(0));
    assert_eq!(Stance::Neutral.as_str(), Some("neutral"));
    assert_eq!(Stance::Neutral.to_value(), Some(40));
}

#[test]
fn test_mind_to_str_and_value() {
    assert_eq!(MindState::Clear.as_str(), "clear");
    assert_eq!(MindState::Clear.to_value(), 0);
    assert_eq!(MindState::Awakening.as_str(), "awakening");
    assert_eq!(MindState::Awakening.to_value(), 10);
    assert_eq!(MindState::Stunned.as_str(), "stunned");
    assert_eq!(MindState::Stunned.to_value(), 100);
    assert_eq!(MindState::BecomingFuzzy.as_str(), "becoming fuzzy");
    assert_eq!(MindState::BecomingFuzzy.to_value(), 65);
}

#[test]
fn test_encumbrance_to_str_and_value() {
    assert_eq!(EncumbranceState::None.as_str(), "none");
    assert_eq!(EncumbranceState::None.to_value(), 0);
    assert_eq!(EncumbranceState::Overburdened.as_str(), "overburdened");
    assert_eq!(EncumbranceState::Overburdened.to_value(), 5);
    assert_eq!(EncumbranceState::VeryHeavy.as_str(), "very heavy");
    assert_eq!(EncumbranceState::VeryHeavy.to_value(), 4);
}

#[test]
fn test_game_to_str() {
    assert_eq!(Game::GemStone.as_str(), "GS");
    assert_eq!(Game::DragonRealms.as_str(), "DR");
}

#[test]
fn test_room_count_increments_on_room_id() {
    let mut gs = GameState::default();
    assert_eq!(gs.room_count, 0);
    gs.apply(XmlEvent::RoomId { id: 1 });
    assert_eq!(gs.room_count, 1);
    gs.apply(XmlEvent::RoomId { id: 2 });
    assert_eq!(gs.room_count, 2);
}

#[test]
fn test_apply_experience() {
    let mut gs = GameState::default();
    gs.apply(XmlEvent::Experience { value: 54321 });
    assert_eq!(gs.experience, 54321);
}

#[test]
fn test_apply_indicator_standing() {
    let mut gs = GameState::default();
    assert!(!gs.standing);
    gs.apply(XmlEvent::Indicator { name: "IconSTANDING".into(), visible: true });
    assert!(gs.standing);
    gs.apply(XmlEvent::Indicator { name: "IconSTANDING".into(), visible: false });
    assert!(!gs.standing);
}

#[test]
fn test_apply_indicator_poisoned() {
    let mut gs = GameState::default();
    gs.apply(XmlEvent::Indicator { name: "IconPOISONED".into(), visible: true });
    assert!(gs.poisoned);
}

#[test]
fn test_apply_indicator_diseased() {
    let mut gs = GameState::default();
    gs.apply(XmlEvent::Indicator { name: "IconDISEASED".into(), visible: true });
    assert!(gs.diseased);
}

#[test]
fn test_apply_indicator_hidden() {
    let mut gs = GameState::default();
    gs.apply(XmlEvent::Indicator { name: "IconHIDDEN".into(), visible: true });
    assert!(gs.hidden);
}

#[test]
fn test_apply_indicator_invisible() {
    let mut gs = GameState::default();
    gs.apply(XmlEvent::Indicator { name: "IconINVISIBLE".into(), visible: true });
    assert!(gs.invisible);
}

#[test]
fn test_apply_indicator_webbed() {
    let mut gs = GameState::default();
    gs.apply(XmlEvent::Indicator { name: "IconWEBBED".into(), visible: true });
    assert!(gs.webbed);
}

#[test]
fn test_apply_indicator_joined() {
    let mut gs = GameState::default();
    gs.apply(XmlEvent::Indicator { name: "IconJOINED".into(), visible: true });
    assert!(gs.joined);
}

#[test]
fn test_new_indicators_default_false() {
    let gs = GameState::default();
    assert!(!gs.standing);
    assert!(!gs.poisoned);
    assert!(!gs.diseased);
    assert!(!gs.hidden);
    assert!(!gs.invisible);
    assert!(!gs.webbed);
    assert!(!gs.joined);
    assert!(!gs.calmed);
    assert!(!gs.cutthroat);
    assert!(!gs.silenced);
    assert!(!gs.bound);
}
