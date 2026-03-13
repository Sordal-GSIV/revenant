use revenant::game_state::GameState;
use std::time::{Duration, Instant};

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
