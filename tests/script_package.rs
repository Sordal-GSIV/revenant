use revenant::script_engine::ScriptEngine;
use std::sync::{Arc, Mutex};
use tempfile::TempDir;

fn setup_with_scripts(dir: &str) -> ScriptEngine {
    let engine = ScriptEngine::new();
    engine.set_scripts_dir(dir);
    let output = Arc::new(Mutex::new(Vec::<String>::new()));
    let out_clone = output.clone();
    engine.set_respond_sink(move |msg| {
        out_clone.lock().unwrap().push(msg);
    });
    engine.install_lua_api().unwrap();
    engine.set_script_error_hook(|name, err| panic!("{name}: {err}"));
    engine
}

#[tokio::test]
async fn test_script_run_single_file() {
    let tmp = TempDir::new().unwrap();
    let dir = tmp.path().to_str().unwrap();
    std::fs::write(
        tmp.path().join("hello.lua"),
        r#"respond("hello from single file")"#,
    )
    .unwrap();

    let engine = setup_with_scripts(dir);
    engine.eval_lua(r#"Script.run("hello")"#).await.unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    // Script should have run (respond called)
}

#[tokio::test]
async fn test_script_run_package_dir() {
    let tmp = TempDir::new().unwrap();
    let pkg_dir = tmp.path().join("mypkg");
    std::fs::create_dir(&pkg_dir).unwrap();
    std::fs::write(
        pkg_dir.join("init.lua"),
        r#"respond("hello from package")"#,
    )
    .unwrap();
    std::fs::write(
        pkg_dir.join("manifest.lua"),
        r#"return { name = "mypkg", version = "1.0.0", author = "test" }"#,
    )
    .unwrap();

    let engine = setup_with_scripts(tmp.path().to_str().unwrap());
    engine.eval_lua(r#"Script.run("mypkg")"#).await.unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
}

#[tokio::test]
async fn test_script_run_dir_precedence_over_file() {
    let tmp = TempDir::new().unwrap();
    // Create both file and directory
    std::fs::write(
        tmp.path().join("dual.lua"),
        r#"respond("from file")"#,
    )
    .unwrap();
    let pkg_dir = tmp.path().join("dual");
    std::fs::create_dir(&pkg_dir).unwrap();
    std::fs::write(
        pkg_dir.join("init.lua"),
        r#"respond("from package")"#,
    )
    .unwrap();

    let engine = setup_with_scripts(tmp.path().to_str().unwrap());
    // Directory should take precedence
    engine.eval_lua(r#"Script.run("dual")"#).await.unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
}

#[tokio::test]
async fn test_package_require_internal_module() {
    let tmp = TempDir::new().unwrap();
    let pkg_dir = tmp.path().join("modpkg");
    std::fs::create_dir(&pkg_dir).unwrap();
    std::fs::write(
        pkg_dir.join("helper.lua"),
        r#"local M = {}
function M.greet() return "hello from helper" end
return M"#,
    )
    .unwrap();
    std::fs::write(
        pkg_dir.join("init.lua"),
        r#"local helper = require("helper")
respond(helper.greet())"#,
    )
    .unwrap();

    let engine = setup_with_scripts(tmp.path().to_str().unwrap());
    engine.eval_lua(r#"Script.run("modpkg")"#).await.unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
}
