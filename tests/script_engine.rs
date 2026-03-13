use revenant::script_engine::ScriptEngine;
use std::sync::{Arc, Mutex};

#[tokio::test]
async fn test_char_settings_roundtrip_from_lua() {
    use revenant::db::Db;
    let engine = ScriptEngine::new();
    engine.set_db(Db::open(":memory:").unwrap(), "TestChar", "GS3");
    engine.install_lua_api().unwrap();  // sync, no .await

    // String value
    engine.eval_lua(r#"CharSettings["autoloot"] = "true""#).await.unwrap();
    engine.eval_lua(r#"assert(CharSettings["autoloot"] == "true")"#).await.unwrap();

    // Missing key returns nil (not empty string)
    engine.eval_lua(r#"assert(CharSettings["nonexistent"] == nil)"#).await.unwrap();
}

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

#[tokio::test]
async fn test_script_run_and_kill() {
    let engine = ScriptEngine::new();
    let tmp = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(tmp.path(), "pause(9999)").unwrap();

    engine.install_lua_api().unwrap();  // sync, no .await
    engine.start_script("s", tmp.path().to_str().unwrap()).unwrap();
    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
    assert!(engine.is_running("s"), "script should be running");
    engine.kill_script("s").await;
    assert!(!engine.is_running("s"));
}

#[tokio::test]
async fn test_script_kill_from_lua() {
    let engine = ScriptEngine::new();
    let tmp = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(tmp.path(), "pause(9999)").unwrap();

    engine.install_lua_api().unwrap();
    engine.start_script("lua_kill_test", tmp.path().to_str().unwrap()).unwrap();
    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
    assert!(engine.is_running("lua_kill_test"));

    // Kill via Lua API (not engine.kill_script directly)
    engine.eval_lua(r#"Script.kill("lua_kill_test")"#).await.unwrap();
    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
    assert!(!engine.is_running("lua_kill_test"));
}
