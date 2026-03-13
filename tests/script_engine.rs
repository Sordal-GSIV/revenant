use revenant::script_engine::ScriptEngine;

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
