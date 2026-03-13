use revenant::script_engine::ScriptEngine;
use std::sync::{Arc, Mutex};

#[tokio::test]
async fn test_engine_executes_simple_lua() {
    let engine = ScriptEngine::new();
    engine.eval_lua("assert(1 + 1 == 2)").await.unwrap();
}

#[tokio::test]
async fn test_engine_print_does_not_panic() {
    let engine = ScriptEngine::new();
    engine.eval_lua("print('hello from test')").await.unwrap();
}

#[tokio::test]
async fn test_put_calls_upstream_sink() {
    let sent: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let engine = ScriptEngine::new();
    let cap = sent.clone();
    engine.set_upstream_sink(move |cmd| { cap.lock().unwrap().push(cmd); });
    engine.install_lua_api().unwrap();  // NOT .await — it's sync
    engine.eval_lua(r#"put("go north")"#).await.unwrap();
    assert_eq!(*sent.lock().unwrap(), vec!["go north\n"]);
}

#[tokio::test]
async fn test_pause_zero_completes() {
    let engine = ScriptEngine::new();
    engine.install_lua_api().unwrap();  // NOT .await — it's sync
    engine.eval_lua("pause(0)").await.unwrap();
}

#[tokio::test]
async fn test_gamestate_health_from_lua() {
    use revenant::game_state::GameState;
    use std::sync::RwLock;
    let gs = Arc::new(RwLock::new(GameState::default()));
    gs.write().unwrap().health = 150;
    gs.write().unwrap().max_health = 200;
    let engine = ScriptEngine::new();
    engine.set_game_state(gs);
    engine.install_lua_api().unwrap();  // sync, no .await
    engine.eval_lua("assert(GameState.health == 150)").await.unwrap();
    engine.eval_lua("assert(GameState.max_health == 200)").await.unwrap();
}

#[tokio::test]
async fn test_gamestate_roundtime_fn() {
    use revenant::game_state::GameState;
    use std::sync::RwLock;
    let gs = Arc::new(RwLock::new(GameState::default()));
    let engine = ScriptEngine::new();
    engine.set_game_state(gs);
    engine.install_lua_api().unwrap();  // sync, no .await
    engine.eval_lua("assert(GameState.roundtime() == 0.0)").await.unwrap();
}

#[tokio::test]
async fn test_downstream_hook_from_lua() {
    let engine = ScriptEngine::new();
    engine.install_lua_api().unwrap();  // sync, no .await
    engine.eval_lua(r#"
        DownstreamHook.add("t", function(line)
            return "[x]" .. line
        end)
    "#).await.unwrap();
    // Verify the hook was registered
    let names = engine.downstream_hooks.lock().unwrap().hook_names();
    assert!(names.contains(&"t".to_string()));
}
