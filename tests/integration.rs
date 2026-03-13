//! v1 acceptance test: healing script sends "drink my potion" when HP < 50%.

use revenant::{
    game_state::GameState,
    script_engine::ScriptEngine,
};
use std::sync::{Arc, Mutex, RwLock};

#[tokio::test]
async fn test_map_find_room_by_id() {
    let e = Arc::new(ScriptEngine::new());
    e.set_upstream_sink(|_| {});
    let map_json = r#"[{"id":10,"title":"The Bank","wayto":{"11":"go north"},"timeto":{"11":0.2},"paths":[],"tags":["bank"]},
                       {"id":11,"title":"Market Street","wayto":{"10":"go south"},"timeto":{"10":0.2},"paths":[],"tags":[]}]"#;
    let map_file = tempfile::NamedTempFile::with_suffix(".json").unwrap();
    std::fs::write(map_file.path(), map_json).unwrap();
    e.load_map(map_file.path().to_str().unwrap()).unwrap();
    e.install_lua_api().unwrap();

    e.eval_lua(r#"
        local r = Map.find_room(10)
        assert(r ~= nil, "room 10 should exist")
        assert(r.id == 10, "id should be 10")
        assert(r.title == "The Bank", "title mismatch")
    "#).await.unwrap();
}

#[tokio::test]
async fn test_map_find_path() {
    let e = Arc::new(ScriptEngine::new());
    e.set_upstream_sink(|_| {});
    let map_json = r#"[{"id":1,"title":"Start","wayto":{"2":"go north"},"timeto":{"2":0.2},"paths":[],"tags":[]},
                       {"id":2,"title":"End","wayto":{"1":"go south"},"timeto":{"1":0.2},"paths":[],"tags":[]}]"#;
    let map_file = tempfile::NamedTempFile::with_suffix(".json").unwrap();
    std::fs::write(map_file.path(), map_json).unwrap();
    e.load_map(map_file.path().to_str().unwrap()).unwrap();
    e.install_lua_api().unwrap();

    e.eval_lua(r#"
        local path = Map.find_path(1, 2)
        assert(path ~= nil, "path should exist")
        assert(#path == 1, "path should have 1 step")
        assert(path[1] == "go north", "step should be 'go north'")
    "#).await.unwrap();
}

const HEALING_SCRIPT: &str = r#"
DownstreamHook.add("auto_heal", function(line)
    if GameState.health < GameState.max_health * 0.5 then
        put("drink my potion")
    end
    return line
end)
"#;

#[tokio::test]
async fn test_healing_hook_sends_drink_on_low_hp() {
    // 1. Set up game state with low HP
    let gs = Arc::new(RwLock::new(GameState::default()));
    {
        let mut state = gs.write().unwrap();
        state.health = 30;
        state.max_health = 100;
    }

    // 2. Set up engine with capture sink
    let sent: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let engine = ScriptEngine::new();
    let cap = sent.clone();
    engine.set_upstream_sink(move |cmd| { cap.lock().unwrap().push(cmd); });
    engine.set_game_state(gs.clone());
    engine.install_lua_api().unwrap();  // sync, no .await

    // 3. Run the healing script (registers the hook)
    engine.eval_lua(HEALING_SCRIPT).await.unwrap();

    // 4. Simulate a downstream line arriving — process through the hook chain.
    // IMPORTANT: acquire lock, call process_with_lua, then drop the lock.
    // The healing hook calls put() but NOT DownstreamHook.add/remove, so no re-entrant lock.
    let result = {
        let chain = engine.downstream_hooks.lock().unwrap();
        chain.process_with_lua(&engine.lua, "The orc attacks you!\n")
            .expect("hook chain failed")
    };
    // Hook should pass the line through (not suppress it)
    assert!(result.is_some(), "line was suppressed unexpectedly");

    // 5. Verify "drink my potion" was sent upstream
    let cmds = sent.lock().unwrap();
    assert!(
        cmds.iter().any(|s| s.contains("drink my potion")),
        "Expected 'drink my potion' in sent commands, got: {cmds:?}"
    );
}

#[tokio::test]
async fn test_healing_hook_does_not_fire_on_full_hp() {
    let gs = Arc::new(RwLock::new(GameState::default()));
    {
        let mut state = gs.write().unwrap();
        state.health = 100;
        state.max_health = 100;
    }

    let sent: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let engine = ScriptEngine::new();
    let cap = sent.clone();
    engine.set_upstream_sink(move |cmd| { cap.lock().unwrap().push(cmd); });
    engine.set_game_state(gs);
    engine.install_lua_api().unwrap();  // sync, no .await
    engine.eval_lua(HEALING_SCRIPT).await.unwrap();

    {
        let chain = engine.downstream_hooks.lock().unwrap();
        chain.process_with_lua(&engine.lua, "The orc attacks you!\n").unwrap();
    }

    let cmds = sent.lock().unwrap();
    assert!(
        cmds.is_empty(),
        "Expected no commands at full HP, got: {cmds:?}"
    );
}
