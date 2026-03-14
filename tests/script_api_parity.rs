use revenant::script_engine::ScriptEngine;
use std::sync::{Arc, Mutex};

#[tokio::test]
async fn test_get_receives_downstream_line() {
    let engine = ScriptEngine::new();
    let (tx, _rx) = tokio::sync::broadcast::channel::<Arc<Vec<u8>>>(64);
    engine.set_downstream_channel(tx.clone());
    engine.install_lua_api().unwrap();

    let errors: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let err_cap = errors.clone();
    engine.set_script_error_hook(move |name, err| {
        err_cap.lock().unwrap().push(format!("{name}: {err}"));
    });

    let tmp = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(tmp.path(), r#"
        local line = get()
        assert(line == "You see a goblin.", "expected goblin, got: " .. tostring(line))
    "#).unwrap();

    engine.start_script("gettest", tmp.path().to_str().unwrap(), vec![]).unwrap();
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    // Send a line through the downstream channel
    tx.send(Arc::new(b"You see a goblin.\n".to_vec())).unwrap();
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    let errs = errors.lock().unwrap();
    assert!(errs.is_empty(), "script errors: {:?}", *errs);
    assert!(!engine.is_running("gettest"), "script should have finished");
}

#[tokio::test]
async fn test_get_noblock_returns_nil_when_empty() {
    let engine = ScriptEngine::new();
    let (tx, _rx) = tokio::sync::broadcast::channel::<Arc<Vec<u8>>>(64);
    engine.set_downstream_channel(tx.clone());
    engine.install_lua_api().unwrap();

    let errors: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let err_cap = errors.clone();
    engine.set_script_error_hook(move |name, err| {
        err_cap.lock().unwrap().push(format!("{name}: {err}"));
    });

    let tmp = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(tmp.path(), r#"
        local line = get_noblock()
        assert(line == nil, "expected nil, got: " .. tostring(line))
    "#).unwrap();

    engine.start_script("nget_test", tmp.path().to_str().unwrap(), vec![]).unwrap();
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    let errs = errors.lock().unwrap();
    assert!(errs.is_empty(), "script errors: {:?}", *errs);
}

#[tokio::test]
async fn test_echo_prefixes_script_name() {
    let responded: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let engine = ScriptEngine::new();
    let cap = responded.clone();
    engine.set_respond_sink(move |msg| { cap.lock().unwrap().push(msg); });
    engine.install_lua_api().unwrap();

    let errors: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let err_cap = errors.clone();
    engine.set_script_error_hook(move |name, err| {
        err_cap.lock().unwrap().push(format!("{name}: {err}"));
    });

    let tmp = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(tmp.path(), r#"echo("hello world")"#).unwrap();

    engine.start_script("myecho", tmp.path().to_str().unwrap(), vec![]).unwrap();
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    let errs = errors.lock().unwrap();
    assert!(errs.is_empty(), "script errors: {:?}", *errs);
    let msgs = responded.lock().unwrap();
    assert!(msgs.iter().any(|m| m.contains("[myecho]: hello world")),
        "expected echo with script prefix, got: {:?}", *msgs);
}

#[tokio::test]
async fn test_reget_returns_recent_lines() {
    let engine = ScriptEngine::new();
    engine.install_lua_api().unwrap();

    // Pre-populate game_log
    {
        let mut log = engine.game_log.lock().unwrap();
        log.push_back("line one".to_string());
        log.push_back("line two".to_string());
        log.push_back("line three".to_string());
    }

    engine.eval_lua(r#"
        local lines = reget(2)
        assert(#lines == 2, "expected 2 lines, got " .. #lines)
        assert(lines[1] == "line two", "expected 'line two', got: " .. lines[1])
        assert(lines[2] == "line three", "expected 'line three', got: " .. lines[2])
    "#).await.unwrap();
}

#[tokio::test]
async fn test_clear_drains_line_buffer() {
    let engine = ScriptEngine::new();
    let (tx, _rx) = tokio::sync::broadcast::channel::<Arc<Vec<u8>>>(64);
    engine.set_downstream_channel(tx.clone());
    engine.install_lua_api().unwrap();

    let errors: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let err_cap = errors.clone();
    engine.set_script_error_hook(move |name, err| {
        err_cap.lock().unwrap().push(format!("{name}: {err}"));
    });

    let tmp = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(tmp.path(), r#"
        pause(0.1)  -- let lines buffer
        local drained = clear()
        assert(type(drained) == "table", "clear should return table")
        assert(#drained >= 2, "expected at least 2 drained lines, got: " .. #drained)
        -- After clear, get_noblock should return nil
        local next = get_noblock()
        assert(next == nil, "buffer should be empty after clear")
    "#).unwrap();

    engine.start_script("cleartest", tmp.path().to_str().unwrap(), vec![]).unwrap();
    tokio::time::sleep(tokio::time::Duration::from_millis(20)).await;

    // Send lines before the script's pause(0.1) expires
    tx.send(Arc::new(b"line A\n".to_vec())).unwrap();
    tx.send(Arc::new(b"line B\n".to_vec())).unwrap();
    tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

    let errs = errors.lock().unwrap();
    assert!(errs.is_empty(), "script errors: {:?}", *errs);
}

#[tokio::test]
async fn test_wait_clears_and_returns_next_line() {
    let engine = ScriptEngine::new();
    let (tx, _rx) = tokio::sync::broadcast::channel::<Arc<Vec<u8>>>(64);
    engine.set_downstream_channel(tx.clone());
    engine.install_lua_api().unwrap();

    let errors: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let err_cap = errors.clone();
    engine.set_script_error_hook(move |name, err| {
        err_cap.lock().unwrap().push(format!("{name}: {err}"));
    });

    let tmp = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(tmp.path(), r#"
        pause(0.1)  -- let stale line buffer before calling wait()
        local line = wait()
        -- wait() clears buffer first, then returns next fresh line
        assert(line == "fresh line", "expected 'fresh line', got: " .. tostring(line))
    "#).unwrap();

    engine.start_script("waittest", tmp.path().to_str().unwrap(), vec![]).unwrap();
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    // Send "stale" line — script is still in pause(0.1), so this buffers
    tx.send(Arc::new(b"stale line\n".to_vec())).unwrap();
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    // Now script's pause has expired; wait() runs, clears "stale line", blocks on get()
    // Send fresh line
    tx.send(Arc::new(b"fresh line\n".to_vec())).unwrap();
    tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

    let errs = errors.lock().unwrap();
    assert!(errs.is_empty(), "script errors: {:?}", *errs);
}

#[tokio::test]
async fn test_waitforre_matches_pattern() {
    let engine = ScriptEngine::new();
    let (tx, _rx) = tokio::sync::broadcast::channel::<Arc<Vec<u8>>>(64);
    engine.set_downstream_channel(tx.clone());
    engine.install_lua_api().unwrap();

    let errors: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let err_cap = errors.clone();
    engine.set_script_error_hook(move |name, err| {
        err_cap.lock().unwrap().push(format!("{name}: {err}"));
    });

    let tmp = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(tmp.path(), r#"
        local line, captures = waitforre("(%d+) gold")
        assert(line == "You have 500 gold coins.", "wrong line: " .. tostring(line))
        assert(captures[1] == "500", "wrong capture: " .. tostring(captures[1]))
    "#).unwrap();

    engine.start_script("wfr", tmp.path().to_str().unwrap(), vec![]).unwrap();
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    tx.send(Arc::new(b"The wind blows.\n".to_vec())).unwrap();
    tokio::time::sleep(tokio::time::Duration::from_millis(20)).await;
    tx.send(Arc::new(b"You have 500 gold coins.\n".to_vec())).unwrap();
    tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

    let errs = errors.lock().unwrap();
    assert!(errs.is_empty(), "script errors: {:?}", *errs);
}

#[tokio::test]
async fn test_waitforre_timeout_returns_nil() {
    let engine = ScriptEngine::new();
    let (tx, _rx) = tokio::sync::broadcast::channel::<Arc<Vec<u8>>>(64);
    engine.set_downstream_channel(tx.clone());
    engine.install_lua_api().unwrap();

    let errors: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let err_cap = errors.clone();
    engine.set_script_error_hook(move |name, err| {
        err_cap.lock().unwrap().push(format!("{name}: {err}"));
    });

    let tmp = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(tmp.path(), r#"
        local line = waitforre("never matches", 2)
        assert(line == nil, "expected nil on timeout, got: " .. tostring(line))
    "#).unwrap();

    engine.start_script("wfr_to", tmp.path().to_str().unwrap(), vec![]).unwrap();
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
    tx.send(Arc::new(b"unrelated text\n".to_vec())).unwrap();
    tokio::time::sleep(tokio::time::Duration::from_millis(2500)).await;

    let errs = errors.lock().unwrap();
    assert!(errs.is_empty(), "script errors: {:?}", *errs);
}

#[tokio::test]
async fn test_matchwait_returns_matching_line() {
    let engine = ScriptEngine::new();
    let (tx, _rx) = tokio::sync::broadcast::channel::<Arc<Vec<u8>>>(64);
    engine.set_downstream_channel(tx.clone());
    engine.install_lua_api().unwrap();

    let errors: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let err_cap = errors.clone();
    engine.set_script_error_hook(move |name, err| {
        err_cap.lock().unwrap().push(format!("{name}: {err}"));
    });

    let tmp = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(tmp.path(), r#"
        local line = matchwait("goblin", "troll")
        assert(string.find(line, "troll"), "expected troll match, got: " .. line)
    "#).unwrap();

    engine.start_script("mw", tmp.path().to_str().unwrap(), vec![]).unwrap();
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    tx.send(Arc::new(b"The wind howls.\n".to_vec())).unwrap();
    tokio::time::sleep(tokio::time::Duration::from_millis(20)).await;
    tx.send(Arc::new(b"A troll appears!\n".to_vec())).unwrap();
    tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

    let errs = errors.lock().unwrap();
    assert!(errs.is_empty(), "script errors: {:?}", *errs);
}

#[tokio::test]
async fn test_matchtimeout_returns_nil_on_timeout() {
    let engine = ScriptEngine::new();
    let (tx, _rx) = tokio::sync::broadcast::channel::<Arc<Vec<u8>>>(64);
    engine.set_downstream_channel(tx.clone());
    engine.install_lua_api().unwrap();

    let errors: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let err_cap = errors.clone();
    engine.set_script_error_hook(move |name, err| {
        err_cap.lock().unwrap().push(format!("{name}: {err}"));
    });

    let tmp = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(tmp.path(), r#"
        local line = matchtimeout(2, "never", "matches")
        assert(line == nil, "expected nil on timeout")
    "#).unwrap();

    engine.start_script("mt", tmp.path().to_str().unwrap(), vec![]).unwrap();
    tokio::time::sleep(tokio::time::Duration::from_millis(2500)).await;

    let errs = errors.lock().unwrap();
    assert!(errs.is_empty(), "script errors: {:?}", *errs);
}

#[tokio::test]
async fn test_matchfind_searches_recent_log() {
    let engine = ScriptEngine::new();
    engine.install_lua_api().unwrap();

    // Pre-populate game_log
    {
        let mut log = engine.game_log.lock().unwrap();
        log.push_back("A goblin snarls.".to_string());
        log.push_back("The troll attacks!".to_string());
        log.push_back("You dodge.".to_string());
    }

    engine.eval_lua(r#"
        local line = matchfind("troll", "dragon")
        assert(line == "The troll attacks!", "expected troll line, got: " .. tostring(line))
    "#).await.unwrap();

    engine.eval_lua(r#"
        local line = matchfind("dragon", "unicorn")
        assert(line == nil, "expected nil for no match")
    "#).await.unwrap();
}

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
