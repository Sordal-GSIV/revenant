use revenant::game_state::GameState;
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
async fn test_room_title() {
    let mut gs = GameState::default();
    gs.room_name = "Town Square".to_string();
    let engine = setup_with_state(gs);
    engine.eval_lua(r#"
        assert(Room.title == "Town Square", "title: " .. tostring(Room.title))
    "#).await.unwrap();
}

#[tokio::test]
async fn test_room_description() {
    let mut gs = GameState::default();
    gs.room_description = "A bustling town square.".to_string();
    let engine = setup_with_state(gs);
    engine.eval_lua(r#"
        assert(Room.description == "A bustling town square.")
    "#).await.unwrap();
}

#[tokio::test]
async fn test_room_exits() {
    let mut gs = GameState::default();
    gs.room_exits = vec!["north".to_string(), "south".to_string(), "out".to_string()];
    let engine = setup_with_state(gs);
    engine.eval_lua(r#"
        local exits = Room.exits
        assert(type(exits) == "table")
        assert(#exits == 3)
        assert(exits[1] == "north")
        assert(exits[2] == "south")
        assert(exits[3] == "out")
    "#).await.unwrap();
}

#[tokio::test]
async fn test_room_id_present() {
    let mut gs = GameState::default();
    gs.room_id = Some(12345);
    let engine = setup_with_state(gs);
    engine.eval_lua(r#"
        assert(Room.id == 12345)
    "#).await.unwrap();
}

#[tokio::test]
async fn test_room_id_nil() {
    let gs = GameState::default();
    let engine = setup_with_state(gs);
    engine.eval_lua(r#"
        assert(Room.id == nil)
    "#).await.unwrap();
}

#[tokio::test]
async fn test_room_count() {
    let mut gs = GameState::default();
    gs.room_count = 42;
    let engine = setup_with_state(gs);
    engine.eval_lua(r#"
        assert(Room.count == 42)
    "#).await.unwrap();
}
