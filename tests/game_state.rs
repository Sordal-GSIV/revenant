use revenant::game_state::GameState;

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
