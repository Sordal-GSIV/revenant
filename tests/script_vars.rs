use revenant::script_engine::ScriptEngine;
use std::sync::{Arc, Mutex};

fn engine() -> Arc<ScriptEngine> {
    let e = Arc::new(ScriptEngine::new());
    e.set_upstream_sink(|_| {});
    e.install_lua_api().unwrap();
    e
}

/// Helper: run a Lua script and assert it completes without error.
/// Uses a shared error flag to surface Lua assertion failures.
async fn run_and_assert(name: &str, code: &str, args: Vec<String>) {
    let e = engine();
    let errors: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(vec![]));
    let errors2 = errors.clone();
    e.set_script_error_hook(move |n, err| {
        errors2.lock().unwrap().push(format!("[{n}] {err}"));
    });

    let tmp = tempfile::NamedTempFile::with_suffix(".lua").unwrap();
    std::fs::write(tmp.path(), code).unwrap();
    e.start_script(name, tmp.path().to_str().unwrap(), args).unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    let errs = errors.lock().unwrap().clone();
    assert!(errs.is_empty(), "Lua script errors: {:?}", errs);
    assert!(!e.is_running(name), "script should have completed");
}

#[tokio::test]
async fn test_script_vars_accessible() {
    run_and_assert(
        "test_vars",
        r#"
        local v = Script.vars
        assert(v[1] == "bank", "expected 'bank', got: " .. tostring(v[1]))
        assert(v[2] == "fast", "expected 'fast', got: " .. tostring(v[2]))
        assert(v[0] == "bank fast", "expected full string in v[0]")
    "#,
        vec![
            "bank fast".to_string(),
            "bank".to_string(),
            "fast".to_string(),
        ],
    )
    .await;
}

#[tokio::test]
async fn test_script_name_accessible() {
    run_and_assert(
        "my_script",
        r#"
        assert(Script.name == "my_script", "expected 'my_script', got: " .. tostring(Script.name))
    "#,
        vec![],
    )
    .await;
}

#[tokio::test]
async fn test_script_run_from_lua() {
    let e = engine();
    let errors: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(vec![]));
    let errors2 = errors.clone();
    e.set_script_error_hook(move |n, err| {
        errors2.lock().unwrap().push(format!("[{n}] {err}"));
    });

    let scripts_dir = tempfile::tempdir().unwrap();
    e.set_scripts_dir(scripts_dir.path().to_str().unwrap());

    // Write a target script
    std::fs::write(
        scripts_dir.path().join("target.lua"),
        r#"
        assert(Script.vars[1] == "hello")
    "#,
    )
    .unwrap();

    // Launch a script that calls Script.run
    let tmp = tempfile::NamedTempFile::with_suffix(".lua").unwrap();
    std::fs::write(
        tmp.path(),
        r#"
        Script.run("target", "hello")
    "#,
    )
    .unwrap();
    e.start_script("launcher", tmp.path().to_str().unwrap(), vec![])
        .unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    let errs = errors.lock().unwrap().clone();
    assert!(errs.is_empty(), "Lua script errors: {:?}", errs);
    assert!(!e.is_running("target"));
}
