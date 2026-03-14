use revenant::game_state::{GameState, Stance, EncumbranceState};
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
async fn test_char_name() {
    let mut gs = GameState::default();
    gs.name = "Sordal".to_string();
    let engine = setup_with_state(gs);
    engine.eval_lua(r#"
        assert(Char.name == "Sordal", "name: " .. tostring(Char.name))
    "#).await.unwrap();
}

#[tokio::test]
async fn test_char_vitals() {
    let mut gs = GameState::default();
    gs.health = 85; gs.max_health = 100;
    gs.mana = 50; gs.max_mana = 200;
    gs.spirit = 10; gs.max_spirit = 20;
    gs.stamina = 75; gs.max_stamina = 100;
    let engine = setup_with_state(gs);
    engine.eval_lua(r#"
        assert(Char.health == 85)
        assert(Char.max_health == 100)
        assert(Char.percent_health == 85)
        assert(Char.mana == 50)
        assert(Char.max_mana == 200)
        assert(Char.percent_mana == 25)
        assert(Char.spirit == 10)
        assert(Char.percent_spirit == 50)
        assert(Char.stamina == 75)
        assert(Char.percent_stamina == 75)
    "#).await.unwrap();
}

#[tokio::test]
async fn test_char_percent_zero_max() {
    let gs = GameState::default(); // all zeros
    let engine = setup_with_state(gs);
    engine.eval_lua(r#"
        assert(Char.percent_health == 0)
        assert(Char.percent_mana == 0)
    "#).await.unwrap();
}

#[tokio::test]
async fn test_char_stance_and_encumbrance() {
    let mut gs = GameState::default();
    gs.stance = Stance::Guarded;
    gs.encumbrance = EncumbranceState::Moderate;
    let engine = setup_with_state(gs);
    engine.eval_lua(r#"
        assert(Char.stance == "guarded")
        assert(Char.stance_value == 20)
        assert(Char.encumbrance == "moderate")
        assert(Char.encumbrance_value == 2)
    "#).await.unwrap();
}

#[tokio::test]
async fn test_char_stance_none_is_nil() {
    let gs = GameState::default();
    let engine = setup_with_state(gs);
    engine.eval_lua(r#"
        assert(Char.stance == nil)
        assert(Char.stance_value == nil)
    "#).await.unwrap();
}

#[tokio::test]
async fn test_char_status_booleans() {
    let mut gs = GameState::default();
    gs.dead = true;
    gs.stunned = true;
    gs.bleeding = false;
    let engine = setup_with_state(gs);
    engine.eval_lua(r#"
        assert(Char.dead == true)
        assert(Char.stunned == true)
        assert(Char.bleeding == false)
        assert(Char.sleeping == false)
        assert(Char.prone == false)
        assert(Char.sitting == false)
        assert(Char.kneeling == false)
    "#).await.unwrap();
}

#[tokio::test]
async fn test_char_level_and_experience() {
    let mut gs = GameState::default();
    gs.level = 42;
    gs.experience = 99999;
    let engine = setup_with_state(gs);
    engine.eval_lua(r#"
        assert(Char.level == 42)
        assert(Char.experience == 99999)
    "#).await.unwrap();
}

#[tokio::test]
async fn test_char_roundtime() {
    let gs = GameState::default(); // no roundtime set
    let engine = setup_with_state(gs);
    engine.eval_lua(r#"
        local rt = Char.roundtime()
        assert(rt == 0.0, "rt: " .. tostring(rt))
        local crt = Char.cast_roundtime()
        assert(crt == 0.0)
    "#).await.unwrap();
}
