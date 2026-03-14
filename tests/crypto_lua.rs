use revenant::script_engine::ScriptEngine;

fn setup() -> ScriptEngine {
    let engine = ScriptEngine::new();
    engine.install_lua_api().unwrap();
    engine
}

#[tokio::test]
async fn test_sha256_string() {
    let engine = setup();
    engine.set_script_error_hook(|name, err| panic!("{name}: {err}"));
    engine
        .eval_lua(
            r#"
            local hash = Crypto.sha256("hello world")
            -- known SHA256 of "hello world"
            assert(hash == "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9",
                   "unexpected hash: " .. hash)
            "#,
        )
        .await
        .unwrap();
}

#[tokio::test]
async fn test_sha256_empty() {
    let engine = setup();
    engine.set_script_error_hook(|name, err| panic!("{name}: {err}"));
    engine
        .eval_lua(
            r#"
            local hash = Crypto.sha256("")
            assert(hash == "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
                   "unexpected hash: " .. hash)
            "#,
        )
        .await
        .unwrap();
}

#[tokio::test]
async fn test_sha256_file() {
    let engine = ScriptEngine::new();
    engine.set_scripts_dir("/tmp/revenant_test_crypto");
    engine.install_lua_api().unwrap();
    engine.set_script_error_hook(|name, err| panic!("{name}: {err}"));
    // Create a test file first
    std::fs::create_dir_all("/tmp/revenant_test_crypto").unwrap();
    std::fs::write("/tmp/revenant_test_crypto/test.txt", "hello world").unwrap();
    engine
        .eval_lua(
            r#"
            local hash, err = Crypto.sha256_file("test.txt")
            assert(hash ~= nil, "expected hash, got nil: " .. tostring(err))
            assert(hash == "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9",
                   "unexpected hash: " .. hash)
            "#,
        )
        .await
        .unwrap();
    std::fs::remove_dir_all("/tmp/revenant_test_crypto").unwrap();
}
