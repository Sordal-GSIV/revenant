use revenant::script_engine::ScriptEngine;
use tempfile::TempDir;

fn setup_with_dir(dir: &str) -> ScriptEngine {
    let engine = ScriptEngine::new();
    engine.set_scripts_dir(dir);
    engine.install_lua_api().unwrap();
    engine.set_script_error_hook(|name, err| panic!("{name}: {err}"));
    engine
}

#[tokio::test]
async fn test_file_write_and_read() {
    let tmp = TempDir::new().unwrap();
    let engine = setup_with_dir(tmp.path().to_str().unwrap());
    engine
        .eval_lua(
            r#"
            local ok = File.write("test.txt", "hello world")
            assert(ok == true, "write failed")
            local content = File.read("test.txt")
            assert(content == "hello world", "content mismatch: " .. tostring(content))
            "#,
        )
        .await
        .unwrap();
}

#[tokio::test]
async fn test_file_exists_and_remove() {
    let tmp = TempDir::new().unwrap();
    let engine = setup_with_dir(tmp.path().to_str().unwrap());
    engine
        .eval_lua(
            r#"
            File.write("exists_test.txt", "data")
            assert(File.exists("exists_test.txt") == true)
            File.remove("exists_test.txt")
            assert(File.exists("exists_test.txt") == false)
            "#,
        )
        .await
        .unwrap();
}

#[tokio::test]
async fn test_file_mkdir_and_list() {
    let tmp = TempDir::new().unwrap();
    let engine = setup_with_dir(tmp.path().to_str().unwrap());
    engine
        .eval_lua(
            r#"
            File.mkdir("subdir")
            assert(File.is_dir("subdir") == true)
            File.write("subdir/a.lua", "-- a")
            File.write("subdir/b.lua", "-- b")
            local files = File.list("subdir")
            assert(#files == 2, "expected 2 files, got " .. #files)
            "#,
        )
        .await
        .unwrap();
}

#[tokio::test]
async fn test_file_sandbox_escape() {
    let tmp = TempDir::new().unwrap();
    let engine = setup_with_dir(tmp.path().to_str().unwrap());
    engine
        .eval_lua(
            r#"
            local content, err = File.read("../../../etc/passwd")
            assert(content == nil, "should have been blocked")
            assert(err ~= nil, "should have error message")
            "#,
        )
        .await
        .unwrap();
}

#[tokio::test]
async fn test_file_mtime() {
    let tmp = TempDir::new().unwrap();
    let engine = setup_with_dir(tmp.path().to_str().unwrap());
    engine
        .eval_lua(
            r#"
            File.write("mtime_test.txt", "data")
            local t, err = File.mtime("mtime_test.txt")
            assert(t ~= nil, "mtime failed: " .. tostring(err))
            assert(type(t) == "number", "expected number, got " .. type(t))
            assert(t > 1700000000, "timestamp too old: " .. tostring(t))
            "#,
        )
        .await
        .unwrap();
}

#[tokio::test]
async fn test_file_read_nonexistent() {
    let tmp = TempDir::new().unwrap();
    let engine = setup_with_dir(tmp.path().to_str().unwrap());
    engine
        .eval_lua(
            r#"
            local content, err = File.read("no_such_file.txt")
            assert(content == nil)
            assert(err ~= nil)
            "#,
        )
        .await
        .unwrap();
}
