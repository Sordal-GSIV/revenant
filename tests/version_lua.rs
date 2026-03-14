use revenant::script_engine::ScriptEngine;

fn setup() -> ScriptEngine {
    let engine = ScriptEngine::new();
    engine.install_lua_api().unwrap();
    engine
}

#[tokio::test]
async fn test_version_parse() {
    let engine = setup();
    engine.set_script_error_hook(|name, err| panic!("{name}: {err}"));
    engine
        .eval_lua(
            r#"
            local v = Version.parse("1.2.3")
            assert(v.major == 1, "major")
            assert(v.minor == 2, "minor")
            assert(v.patch == 3, "patch")
            assert(v.pre == nil or v.pre == "", "pre should be empty")
            "#,
        )
        .await
        .unwrap();
}

#[tokio::test]
async fn test_version_parse_prerelease() {
    let engine = setup();
    engine.set_script_error_hook(|name, err| panic!("{name}: {err}"));
    engine
        .eval_lua(
            r#"
            local v = Version.parse("1.3.0-beta.1")
            assert(v.major == 1)
            assert(v.minor == 3)
            assert(v.patch == 0)
            assert(v.pre == "beta.1", "pre was: " .. tostring(v.pre))
            "#,
        )
        .await
        .unwrap();
}

#[tokio::test]
async fn test_version_compare() {
    let engine = setup();
    engine.set_script_error_hook(|name, err| panic!("{name}: {err}"));
    engine
        .eval_lua(
            r#"
            assert(Version.compare("1.0.0", "2.0.0") == -1)
            assert(Version.compare("2.0.0", "1.0.0") == 1)
            assert(Version.compare("1.0.0", "1.0.0") == 0)
            assert(Version.compare("1.0.0-beta.1", "1.0.0") == -1)
            "#,
        )
        .await
        .unwrap();
}

#[tokio::test]
async fn test_version_satisfies() {
    let engine = setup();
    engine.set_script_error_hook(|name, err| panic!("{name}: {err}"));
    engine
        .eval_lua(
            r#"
            assert(Version.satisfies("1.2.0", ">= 1.0") == true)
            assert(Version.satisfies("0.9.0", ">= 1.0") == false)
            assert(Version.satisfies("1.5.0", ">= 1.0, < 2.0") == true)
            assert(Version.satisfies("2.1.0", ">= 1.0, < 2.0") == false)
            "#,
        )
        .await
        .unwrap();
}
