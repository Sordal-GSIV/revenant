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

    // Boolean value coerced to string
    engine.eval_lua(r#"CharSettings["flag"] = true"#).await.unwrap();
    engine.eval_lua(r#"assert(CharSettings["flag"] == "true")"#).await.unwrap();

    // Number coerced to string
    engine.eval_lua(r#"CharSettings["threshold"] = 42"#).await.unwrap();
    engine.eval_lua(r#"assert(CharSettings["threshold"] == "42")"#).await.unwrap();
}

#[tokio::test]
async fn test_user_vars_roundtrip_from_lua() {
    use revenant::db::Db;
    let engine = ScriptEngine::new();
    engine.set_db(Db::open(":memory:").unwrap(), "TestChar", "GS3");
    engine.install_lua_api().unwrap();  // sync, no .await

    engine.eval_lua(r#"UserVars["threshold"] = "500""#).await.unwrap();
    engine.eval_lua(r#"assert(UserVars["threshold"] == "500")"#).await.unwrap();

    // Missing key returns nil
    engine.eval_lua(r#"assert(UserVars["nonexistent"] == nil)"#).await.unwrap();
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
    engine.start_script("s", tmp.path().to_str().unwrap(), vec![]).unwrap();
    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
    assert!(engine.is_running("s"), "script should be running");
    engine.kill_script("s").await;
    assert!(!engine.is_running("s"));
}

#[tokio::test]
async fn test_settings_global_roundtrip() {
    use revenant::db::Db;
    let engine = ScriptEngine::new();
    engine.set_db(Db::open(":memory:").unwrap(), "TestChar", "GS3");
    engine.install_lua_api().unwrap();

    engine.eval_lua(r#"Settings["my_key"] = "hello""#).await.unwrap();
    engine.eval_lua(r#"assert(Settings["my_key"] == "hello", "roundtrip failed")"#).await.unwrap();
    engine.eval_lua(r#"assert(Settings["missing"] == nil, "missing should be nil")"#).await.unwrap();
    engine.eval_lua(r#"Settings["flag"] = true"#).await.unwrap();
    engine.eval_lua(r#"assert(Settings["flag"] == "true")"#).await.unwrap();
}

#[tokio::test]
async fn test_settings_independent_of_char_settings() {
    use revenant::db::Db;
    let engine = ScriptEngine::new();
    engine.set_db(Db::open(":memory:").unwrap(), "TestChar", "GS3");
    engine.install_lua_api().unwrap();

    engine.eval_lua(r#"Settings["shared"] = "global_val""#).await.unwrap();
    engine.eval_lua(r#"CharSettings["shared"] = "char_val""#).await.unwrap();
    engine.eval_lua(r#"assert(Settings["shared"] == "global_val")"#).await.unwrap();
    engine.eval_lua(r#"assert(CharSettings["shared"] == "char_val")"#).await.unwrap();
}

#[tokio::test]
async fn test_settings_nil_deletes() {
    use revenant::db::Db;
    let engine = ScriptEngine::new();
    engine.set_db(Db::open(":memory:").unwrap(), "TestChar", "GS3");
    engine.install_lua_api().unwrap();

    engine.eval_lua(r#"Settings["temp"] = "value""#).await.unwrap();
    engine.eval_lua(r#"assert(Settings["temp"] == "value")"#).await.unwrap();
    engine.eval_lua(r#"Settings["temp"] = nil"#).await.unwrap();
    engine.eval_lua(r#"assert(Settings["temp"] == nil, "nil assignment should delete key")"#).await.unwrap();
}

#[tokio::test]
async fn test_script_kill_from_lua() {
    let engine = ScriptEngine::new();
    let tmp = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(tmp.path(), "pause(9999)").unwrap();

    engine.install_lua_api().unwrap();
    engine.start_script("lua_kill_test", tmp.path().to_str().unwrap(), vec![]).unwrap();
    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
    assert!(engine.is_running("lua_kill_test"));

    // Kill via Lua API (not engine.kill_script directly)
    engine.eval_lua(r#"Script.kill("lua_kill_test")"#).await.unwrap();
    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
    assert!(!engine.is_running("lua_kill_test"));
}

#[tokio::test]
async fn test_script_running_returns_false_when_not_running() {
    let engine = ScriptEngine::new();
    engine.install_lua_api().unwrap();
    engine.eval_lua(r#"
        assert(Script.running("nonexistent") == false, "not running script should return false")
    "#).await.unwrap();
}

#[tokio::test]
async fn test_script_running_returns_true_when_running() {
    use tempfile::TempDir;
    let tmp = TempDir::new().unwrap();
    std::fs::write(tmp.path().join("daemon.lua"), b"pause(9999)").unwrap();

    let engine = ScriptEngine::new();
    engine.set_scripts_dir(tmp.path().to_str().unwrap());
    engine.install_lua_api().unwrap();
    engine.set_script_error_hook(|name, err| panic!("{name}: {err}"));

    engine.eval_lua(r#"Script.run("daemon")"#).await.unwrap();
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    engine.eval_lua(r#"
        assert(Script.running("daemon") == true, "daemon should be running")
    "#).await.unwrap();
}
