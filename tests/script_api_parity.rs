use revenant::script_engine::ScriptEngine;
use std::sync::{Arc, Mutex};

#[tokio::test]
async fn test_get_receives_downstream_line() {
    let engine = ScriptEngine::new();
    let (tx, _rx) = tokio::sync::broadcast::channel::<Arc<Vec<u8>>>(64);
    engine.set_downstream_channel(tx.clone());
    engine.install_lua_api().unwrap();

    let errors: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let err_cap = errors.clone();
    engine.set_script_error_hook(move |name, err| {
        err_cap.lock().unwrap().push(format!("{name}: {err}"));
    });

    let tmp = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(tmp.path(), r#"
        local line = get()
        assert(line == "You see a goblin.", "expected goblin, got: " .. tostring(line))
    "#).unwrap();

    engine.start_script("gettest", tmp.path().to_str().unwrap(), vec![]).unwrap();
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    // Send a line through the downstream channel
    tx.send(Arc::new(b"You see a goblin.\n".to_vec())).unwrap();
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    let errs = errors.lock().unwrap();
    assert!(errs.is_empty(), "script errors: {:?}", *errs);
    assert!(!engine.is_running("gettest"), "script should have finished");
}

#[tokio::test]
async fn test_get_noblock_returns_nil_when_empty() {
    let engine = ScriptEngine::new();
    let (tx, _rx) = tokio::sync::broadcast::channel::<Arc<Vec<u8>>>(64);
    engine.set_downstream_channel(tx.clone());
    engine.install_lua_api().unwrap();

    let errors: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let err_cap = errors.clone();
    engine.set_script_error_hook(move |name, err| {
        err_cap.lock().unwrap().push(format!("{name}: {err}"));
    });

    let tmp = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(tmp.path(), r#"
        local line = get_noblock()
        assert(line == nil, "expected nil, got: " .. tostring(line))
    "#).unwrap();

    engine.start_script("nget_test", tmp.path().to_str().unwrap(), vec![]).unwrap();
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    let errs = errors.lock().unwrap();
    assert!(errs.is_empty(), "script errors: {:?}", *errs);
}

#[tokio::test]
async fn test_per_thread_identity_survives_yield() {
    let engine = ScriptEngine::new();
    engine.install_lua_api().unwrap();

    let errors: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let err_cap = errors.clone();
    engine.set_script_error_hook(move |name, err| {
        err_cap.lock().unwrap().push(format!("{name}: {err}"));
    });

    // Script A: sets identity, yields, then checks identity is still "a"
    let tmp_a = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(tmp_a.path(), r#"
        assert(Script.name == "a", "expected Script.name == 'a', got: " .. tostring(Script.name))
        pause(0.05)
        assert(Script.name == "a", "identity lost after yield: " .. tostring(Script.name))
    "#).unwrap();

    // Script B: launched between A's yields to overwrite the global
    let tmp_b = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(tmp_b.path(), r#"
        assert(Script.name == "b", "expected Script.name == 'b', got: " .. tostring(Script.name))
    "#).unwrap();

    engine.start_script("a", tmp_a.path().to_str().unwrap(), vec![]).unwrap();
    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
    engine.start_script("b", tmp_b.path().to_str().unwrap(), vec![]).unwrap();
    tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

    let errs = errors.lock().unwrap();
    assert!(errs.is_empty(), "script errors: {:?}", *errs);
}
