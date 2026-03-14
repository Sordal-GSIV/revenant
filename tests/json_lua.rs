use revenant::script_engine::ScriptEngine;

fn setup() -> ScriptEngine {
    let engine = ScriptEngine::new();
    engine.install_lua_api().unwrap();
    engine
}

#[tokio::test]
async fn test_json_encode_table() {
    let engine = setup();
    engine.set_script_error_hook(|name, err| panic!("{name}: {err}"));
    engine
        .eval_lua(
            r#"
            local t = { name = "test", version = "1.0" }
            local s = Json.encode(t)
            assert(type(s) == "string", "expected string, got " .. type(s))
            local parsed = Json.decode(s)
            assert(parsed.name == "test", "name mismatch")
            assert(parsed.version == "1.0", "version mismatch")
            "#,
        )
        .await
        .unwrap();
}

#[tokio::test]
async fn test_json_encode_array() {
    let engine = setup();
    engine.set_script_error_hook(|name, err| panic!("{name}: {err}"));
    engine
        .eval_lua(
            r#"
            local arr = { "a", "b", "c" }
            local s = Json.encode(arr)
            local parsed = Json.decode(s)
            assert(parsed[1] == "a")
            assert(parsed[2] == "b")
            assert(parsed[3] == "c")
            "#,
        )
        .await
        .unwrap();
}

#[tokio::test]
async fn test_json_decode_invalid() {
    let engine = setup();
    engine.set_script_error_hook(|name, err| panic!("{name}: {err}"));
    engine
        .eval_lua(
            r#"
            local val, err = Json.decode("not json{{{")
            assert(val == nil, "expected nil for invalid json")
            assert(type(err) == "string", "expected error string")
            "#,
        )
        .await
        .unwrap();
}

#[tokio::test]
async fn test_json_encode_nested() {
    let engine = setup();
    engine.set_script_error_hook(|name, err| panic!("{name}: {err}"));
    engine
        .eval_lua(
            r#"
            local t = { outer = { inner = 42 } }
            local s = Json.encode(t)
            local parsed = Json.decode(s)
            assert(parsed.outer.inner == 42)
            "#,
        )
        .await
        .unwrap();
}
