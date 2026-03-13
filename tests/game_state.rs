use revenant::game_state::GameState;
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
