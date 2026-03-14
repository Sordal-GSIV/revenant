use revenant::script_engine::ScriptEngine;

fn setup() -> ScriptEngine {
    let engine = ScriptEngine::new();
    engine.install_lua_api().unwrap();
    engine.set_script_error_hook(|name, err| panic!("{name}: {err}"));
    engine
}

#[tokio::test]
async fn test_http_get_json_public_api() {
    // Uses httpbin.org as a public test endpoint
    let engine = setup();
    engine
        .eval_lua(
            r#"
            local data, err = Http.get_json("https://httpbin.org/get")
            if data == nil then
                -- Network may be unavailable in CI, skip gracefully
                return
            end
            assert(type(data) == "table", "expected table")
            assert(data.url == "https://httpbin.org/get", "url mismatch")
            "#,
        )
        .await
        .unwrap();
}

#[tokio::test]
async fn test_http_get_returns_status() {
    let engine = setup();
    engine
        .eval_lua(
            r#"
            local resp, err = Http.get("https://httpbin.org/status/404")
            if resp == nil then
                -- Network unavailable, skip
                return
            end
            assert(resp.status == 404, "expected 404, got " .. tostring(resp.status))
            "#,
        )
        .await
        .unwrap();
}

#[tokio::test]
async fn test_http_get_unreachable() {
    let engine = setup();
    engine
        .eval_lua(
            r#"
            local resp, err = Http.get("https://192.0.2.1:1/nope")
            assert(resp == nil, "expected nil for unreachable")
            assert(type(err) == "string", "expected error string")
            "#,
        )
        .await
        .unwrap();
}

#[tokio::test]
async fn test_http_get_json_invalid() {
    let engine = setup();
    engine
        .eval_lua(
            r#"
            -- HTML page, not JSON
            local data, err = Http.get_json("https://httpbin.org/html")
            if data == nil and err == nil then
                -- Network unavailable
                return
            end
            assert(data == nil, "expected nil for non-json")
            assert(type(err) == "string", "expected error string")
            "#,
        )
        .await
        .unwrap();
}
