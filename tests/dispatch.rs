use revenant::dispatch::{dispatch, DispatchResult};
use revenant::script_engine::ScriptEngine;
use std::sync::Arc;

fn make_engine() -> Arc<ScriptEngine> {
    let e = Arc::new(ScriptEngine::new());
    e.set_upstream_sink(|_| {});
    e.install_lua_api().unwrap(); // sync — no .await
    e
}

#[tokio::test]
async fn test_passthrough_non_semicolon() {
    let e = make_engine();
    let r = dispatch("go north", &e).await;
    assert!(matches!(r, DispatchResult::Forward(_)));
    if let DispatchResult::Forward(s) = r { assert_eq!(s, "go north"); }
}

#[tokio::test]
async fn test_list_command() {
    let e = make_engine();
    let r = dispatch(";list", &e).await;
    assert!(matches!(r, DispatchResult::Consumed));
}

#[tokio::test]
async fn test_kill_nonexistent_script() {
    let e = make_engine();
    let r = dispatch(";kill nosuchscript", &e).await;
    assert!(matches!(r, DispatchResult::Consumed));
}

#[tokio::test]
async fn test_exec_command() {
    let e = make_engine();
    let r = dispatch(";exec respond('hello')", &e).await;
    assert!(matches!(r, DispatchResult::Consumed));
}

#[tokio::test]
async fn test_script_not_found() {
    let e = make_engine();
    let r = dispatch(";nonexistent_script_xyz", &e).await;
    assert!(matches!(r, DispatchResult::Consumed));
}

#[tokio::test]
async fn test_stormfront_c_prefix_stripped() {
    let e = make_engine();
    let r = dispatch("<c>;list", &e).await;
    assert!(matches!(r, DispatchResult::Consumed));
}
