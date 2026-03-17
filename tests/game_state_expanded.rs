use revenant::game_state::{GameState, Stance, MindState, EncumbranceState, Game, ActiveSpellEntry};
use revenant::script_engine::ScriptEngine;
use std::sync::{Arc, RwLock};

fn setup_with_state(gs: GameState) -> ScriptEngine {
    let engine = ScriptEngine::new();
    let gs_arc = Arc::new(RwLock::new(gs));
    engine.set_game_state(gs_arc);
    engine.install_lua_api().unwrap();
    engine.set_script_error_hook(|name, err| panic!("{name}: {err}"));
    engine
}

#[tokio::test]
async fn test_gamestate_concentration() {
    let mut gs = GameState::default();
    gs.concentration = 50;
    gs.max_concentration = 100;
    let engine = setup_with_state(gs);
    engine.eval_lua(r#"
        assert(GameState.concentration == 50, "conc: " .. tostring(GameState.concentration))
        assert(GameState.max_concentration == 100)
    "#).await.unwrap();
}

#[tokio::test]
async fn test_gamestate_room_fields() {
    let mut gs = GameState::default();
    gs.room_description = "A dark forest clearing.".to_string();
    gs.room_exits = vec!["north".to_string(), "east".to_string()];
    gs.room_id = Some(42);
    gs.room_count = 5;
    let engine = setup_with_state(gs);
    engine.eval_lua(r#"
        assert(GameState.room_description == "A dark forest clearing.")
        local exits = GameState.room_exits
        assert(type(exits) == "table")
        assert(#exits == 2)
        assert(exits[1] == "north")
        assert(exits[2] == "east")
        assert(GameState.room_id == 42)
        assert(GameState.room_count == 5)
    "#).await.unwrap();
}

#[tokio::test]
async fn test_gamestate_room_id_nil() {
    let gs = GameState::default(); // room_id is None
    let engine = setup_with_state(gs);
    engine.eval_lua(r#"
        assert(GameState.room_id == nil)
    "#).await.unwrap();
}

#[tokio::test]
async fn test_gamestate_stance() {
    let mut gs = GameState::default();
    gs.stance = Stance::Offensive;
    let engine = setup_with_state(gs);
    engine.eval_lua(r#"
        assert(GameState.stance == "offensive", "stance: " .. tostring(GameState.stance))
        assert(GameState.stance_value == 100)
    "#).await.unwrap();
}

#[tokio::test]
async fn test_gamestate_stance_none_is_nil() {
    let gs = GameState::default(); // stance is None
    let engine = setup_with_state(gs);
    engine.eval_lua(r#"
        assert(GameState.stance == nil)
        assert(GameState.stance_value == nil)
    "#).await.unwrap();
}

#[tokio::test]
async fn test_gamestate_mind() {
    let mut gs = GameState::default();
    gs.mind = MindState::Muddled;
    let engine = setup_with_state(gs);
    engine.eval_lua(r#"
        assert(GameState.mind == "muddled")
        assert(GameState.mind_value == 60)
    "#).await.unwrap();
}

#[tokio::test]
async fn test_gamestate_encumbrance() {
    let mut gs = GameState::default();
    gs.encumbrance = EncumbranceState::Heavy;
    let engine = setup_with_state(gs);
    engine.eval_lua(r#"
        assert(GameState.encumbrance == "heavy")
        assert(GameState.encumbrance_value == 3)
    "#).await.unwrap();
}

#[tokio::test]
async fn test_gamestate_spells() {
    let mut gs = GameState::default();
    gs.prepared_spell = Some("Spirit Shield".to_string());
    gs.active_spells = vec![
        ActiveSpellEntry { name: "Spirit Shield".to_string(), duration_secs: Some(300), activated_at: std::time::Instant::now() },
        ActiveSpellEntry { name: "Bravery".to_string(), duration_secs: None, activated_at: std::time::Instant::now() },
    ];
    let engine = setup_with_state(gs);
    engine.eval_lua(r#"
        assert(GameState.prepared_spell == "Spirit Shield")
        local spells = GameState.active_spells
        assert(#spells == 2)
        assert(spells[1].name == "Spirit Shield")
        assert(spells[1].duration == 300)
        assert(spells[2].name == "Bravery")
        assert(spells[2].duration == nil)
    "#).await.unwrap();
}

#[tokio::test]
async fn test_gamestate_prepared_spell_nil() {
    let gs = GameState::default();
    let engine = setup_with_state(gs);
    engine.eval_lua(r#"
        assert(GameState.prepared_spell == nil)
    "#).await.unwrap();
}

#[tokio::test]
async fn test_gamestate_misc_fields() {
    let mut gs = GameState::default();
    gs.server_time = 1710000000;
    gs.name = "Sordal".to_string();
    gs.game = Game::GemStone;
    gs.experience = 12345;
    gs.right_hand = Some("sword".to_string());
    gs.left_hand = None;
    let engine = setup_with_state(gs);
    engine.eval_lua(r#"
        assert(GameState.server_time == 1710000000)
        assert(GameState.name == "Sordal")
        assert(GameState.game == "GS3")
        assert(GameState.experience == 12345)
        assert(GameState.right_hand_noun == "sword")
        assert(GameState.left_hand_noun == nil)
    "#).await.unwrap();
}
