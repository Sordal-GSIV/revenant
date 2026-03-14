use revenant::script_engine::ScriptEngine;
use std::sync::{Arc, Mutex};

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
