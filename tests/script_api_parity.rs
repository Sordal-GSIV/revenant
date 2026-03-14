use revenant::script_engine::ScriptEngine;
use std::sync::{Arc, Mutex, RwLock};

#[tokio::test]
async fn test_full_api_loads_without_error() {
    use revenant::game_state::GameState;
    let gs = Arc::new(std::sync::RwLock::new(GameState::default()));
    let engine = ScriptEngine::new();
    engine.set_game_state(gs);
    let (tx, _rx) = tokio::sync::broadcast::channel::<Arc<Vec<u8>>>(64);
    engine.set_downstream_channel(tx);
    engine.install_lua_api().unwrap();

    // Verify all functions exist as globals
    engine.eval_lua(r#"
        assert(type(get) == "function", "get missing")
        assert(type(get_noblock) == "function", "get_noblock missing")
        assert(type(nget) == "function", "nget missing")
        assert(type(echo) == "function", "echo missing")
        assert(type(reget) == "function", "reget missing")
        assert(type(clear) == "function", "clear missing")
        assert(type(wait) == "function", "wait missing")
        assert(type(waitforre) == "function", "waitforre missing")
        assert(type(matchwait) == "function", "matchwait missing")
        assert(type(matchtimeout) == "function", "matchtimeout missing")
        assert(type(matchfind) == "function", "matchfind missing")
        assert(type(fput) == "function", "fput missing")
        assert(type(_raw_fput) == "function", "_raw_fput missing")
        assert(type(multifput) == "function", "multifput missing")
        assert(type(waitrt) == "function", "waitrt missing")
        assert(type(waitcastrt) == "function", "waitcastrt missing")
        assert(type(checkrt) == "function", "checkrt missing")
        assert(type(checkcastrt) == "function", "checkcastrt missing")
        assert(type(wait_until) == "function", "wait_until missing")
        assert(type(wait_while) == "function", "wait_while missing")
        assert(type(move) == "function", "move missing")
        assert(type(n) == "function", "n missing")
        assert(type(s) == "function", "s missing")
        assert(type(e) == "function", "e missing")
        assert(type(w) == "function", "w missing")
        assert(type(before_dying) == "function", "before_dying missing")
        assert(type(undo_before_dying) == "function", "undo_before_dying missing")
        assert(type(no_kill_all) == "function", "no_kill_all missing")
        assert(type(no_pause_all) == "function", "no_pause_all missing")
        assert(type(die_with_me) == "function", "die_with_me missing")
        assert(type(running) == "function", "running missing")
        assert(type(send_to_script) == "function", "send_to_script missing")
        assert(type(health) == "function", "health missing")
        assert(type(mana) == "function", "mana missing")
        assert(type(stunned) == "function", "stunned missing")
        assert(type(room_name) == "function", "room_name missing")
        assert(type(Script.exists) == "function", "Script.exists missing")
        assert(type(Script.at_exit) == "function", "Script.at_exit missing")
        assert(type(Script.clear_exit_procs) == "function", "Script.clear_exit_procs missing")
    "#).await.unwrap();
}

#[tokio::test]
async fn test_before_dying_runs_on_normal_exit() {
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
    std::fs::write(tmp.path(), r#"
        before_dying(function()
            respond("cleanup ran")
        end)
        -- script exits normally
    "#).unwrap();

    engine.start_script("bdtest", tmp.path().to_str().unwrap(), vec![]).unwrap();
    tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

    let msgs = responded.lock().unwrap();
    assert!(msgs.iter().any(|m| m.contains("cleanup ran")),
        "before_dying callback should have run, got: {:?}", *msgs);
}

#[tokio::test]
async fn test_undo_before_dying_clears_hooks() {
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
    std::fs::write(tmp.path(), r#"
        before_dying(function()
            respond("should not run")
        end)
        undo_before_dying()
        -- script exits normally, no hooks should fire
    "#).unwrap();

    engine.start_script("ubdtest", tmp.path().to_str().unwrap(), vec![]).unwrap();
    tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

    let msgs = responded.lock().unwrap();
    assert!(!msgs.iter().any(|m| m.contains("should not run")),
        "undo_before_dying should have cleared hooks");
}

#[tokio::test]
async fn test_checkrt_returns_zero_when_no_roundtime() {
    use revenant::game_state::GameState;
    let gs = Arc::new(RwLock::new(GameState::default()));
    let engine = ScriptEngine::new();
    engine.set_game_state(gs);
    engine.install_lua_api().unwrap();

    engine.eval_lua(r#"
        local rt = checkrt()
        assert(rt == 0, "expected 0, got: " .. rt)
    "#).await.unwrap();
}

#[tokio::test]
async fn test_wait_until_returns_when_condition_met() {
    let engine = ScriptEngine::new();
    engine.install_lua_api().unwrap();

    let start = tokio::time::Instant::now();
    engine.eval_lua(r#"
        local counter = 0
        local result = wait_until(function()
            counter = counter + 1
            return counter >= 3
        end, 0.05)
        assert(result == true, "wait_until should return truthy value")
    "#).await.unwrap();
    let elapsed = start.elapsed();
    assert!(elapsed >= tokio::time::Duration::from_millis(80),
        "should have polled at least twice at 0.05s interval");
}

#[tokio::test]
async fn test_wait_while_returns_when_condition_false() {
    let engine = ScriptEngine::new();
    engine.install_lua_api().unwrap();

    engine.eval_lua(r#"
        local counter = 3
        wait_while(function()
            counter = counter - 1
            return counter > 0
        end, 0.05)
        assert(counter == 0, "wait_while should loop until condition is false")
    "#).await.unwrap();
}

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
async fn test_waitrt_returns_immediately_when_no_roundtime() {
    use revenant::game_state::GameState;
    let gs = Arc::new(RwLock::new(GameState::default()));
    let engine = ScriptEngine::new();
    engine.set_game_state(gs);
    engine.install_lua_api().unwrap();

    // No roundtime set, should return immediately
    let start = tokio::time::Instant::now();
    engine.eval_lua("waitrt()").await.unwrap();
    assert!(start.elapsed() < tokio::time::Duration::from_millis(200),
        "waitrt() should return immediately with no roundtime");
}

#[tokio::test]
async fn test_waitrt_waits_for_roundtime() {
    use revenant::game_state::GameState;
    use std::time::Instant as StdInstant;
    let gs = Arc::new(RwLock::new(GameState::default()));
    // Set roundtime to expire 0.3s from now
    gs.write().unwrap().roundtime_end = Some(StdInstant::now() + std::time::Duration::from_millis(300));
    let engine = ScriptEngine::new();
    engine.set_game_state(gs);
    engine.install_lua_api().unwrap();

    let start = tokio::time::Instant::now();
    engine.eval_lua("waitrt()").await.unwrap();
    let elapsed = start.elapsed();
    assert!(elapsed >= tokio::time::Duration::from_millis(250),
        "waitrt() should wait for roundtime, elapsed: {:?}", elapsed);
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

#[tokio::test]
async fn test_move_sends_direction_and_waits() {
    let sent: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let engine = ScriptEngine::new();
    let cap = sent.clone();
    engine.set_upstream_sink(move |cmd| { cap.lock().unwrap().push(cmd); });

    let (tx, _rx) = tokio::sync::broadcast::channel::<Arc<Vec<u8>>>(64);
    engine.set_downstream_channel(tx.clone());

    use revenant::game_state::GameState;
    let gs = Arc::new(RwLock::new(GameState::default()));
    engine.set_game_state(gs);
    engine.install_lua_api().unwrap();

    let errors: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let err_cap = errors.clone();
    engine.set_script_error_hook(move |name, err| {
        err_cap.lock().unwrap().push(format!("{name}: {err}"));
    });

    let tmp = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(tmp.path(), r#"move("north")"#).unwrap();

    engine.start_script("movetest", tmp.path().to_str().unwrap(), vec![]).unwrap();
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    // Simulate room description indicating success
    tx.send(Arc::new(b"[Town Square]\nObvious exits: south, east\n".to_vec())).unwrap();
    tokio::time::sleep(tokio::time::Duration::from_millis(300)).await;

    let cmds = sent.lock().unwrap();
    assert!(cmds.iter().any(|c| c.contains("north")), "should have sent north");
}

#[tokio::test]
async fn test_raw_fput_still_works() {
    let sent: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let engine = ScriptEngine::new();
    let cap = sent.clone();
    engine.set_upstream_sink(move |cmd| { cap.lock().unwrap().push(cmd); });
    let (tx, _rx) = tokio::sync::broadcast::channel::<Arc<Vec<u8>>>(64);
    engine.set_downstream_channel(tx.clone());
    engine.install_lua_api().unwrap();

    // _raw_fput sends command and waits for prompt
    let tx2 = tx.clone();
    tokio::spawn(async move {
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
        tx2.send(Arc::new(b"<prompt time=\"123\">></prompt>\n".to_vec())).unwrap();
    });

    engine.eval_lua(r#"_raw_fput("look")"#).await.unwrap();
    let cmds = sent.lock().unwrap();
    assert!(cmds.iter().any(|c| c.contains("look")));
}

#[tokio::test]
async fn test_script_exists_finds_lua_file() {
    use tempfile::TempDir;
    let tmp = TempDir::new().unwrap();
    std::fs::write(tmp.path().join("myscript.lua"), b"-- test").unwrap();
    std::fs::create_dir(tmp.path().join("mypkg")).unwrap();
    std::fs::write(tmp.path().join("mypkg").join("init.lua"), b"-- pkg").unwrap();

    let engine = ScriptEngine::new();
    engine.set_scripts_dir(tmp.path().to_str().unwrap());
    engine.install_lua_api().unwrap();

    engine.eval_lua(r#"
        assert(Script.exists("myscript") == true, "myscript.lua should exist")
        assert(Script.exists("mypkg") == true, "mypkg/init.lua should exist")
        assert(Script.exists("nonexistent") == false, "nonexistent should not exist")
    "#).await.unwrap();
}

#[tokio::test]
async fn test_no_kill_all_protects_script() {
    let engine = ScriptEngine::new();
    engine.install_lua_api().unwrap();

    let errors: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let err_cap = errors.clone();
    engine.set_script_error_hook(move |name, err| {
        err_cap.lock().unwrap().push(format!("{name}: {err}"));
    });

    let tmp_protected = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(tmp_protected.path(), r#"
        no_kill_all()
        pause(9999)
    "#).unwrap();

    let tmp_normal = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(tmp_normal.path(), "pause(9999)").unwrap();

    engine.start_script("protected", tmp_protected.path().to_str().unwrap(), vec![]).unwrap();
    engine.start_script("normal", tmp_normal.path().to_str().unwrap(), vec![]).unwrap();
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    assert!(engine.is_running("protected"));
    assert!(engine.is_running("normal"));

    engine.kill_all().await;
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    assert!(engine.is_running("protected"), "protected script should survive kill_all");
    assert!(!engine.is_running("normal"), "normal script should be killed");

    // Cleanup
    engine.kill_script("protected").await;
}

#[tokio::test]
async fn test_die_with_me_kills_target_on_exit() {
    use tempfile::TempDir;
    let tmp = TempDir::new().unwrap();
    std::fs::write(tmp.path().join("parent.lua"), r#"
        die_with_me("child")
        -- parent exits immediately
    "#.as_bytes()).unwrap();
    std::fs::write(tmp.path().join("child.lua"), b"pause(9999)").unwrap();

    let engine = ScriptEngine::new();
    engine.set_scripts_dir(tmp.path().to_str().unwrap());
    engine.install_lua_api().unwrap();

    let errors: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let err_cap = errors.clone();
    engine.set_script_error_hook(move |name, err| {
        err_cap.lock().unwrap().push(format!("{name}: {err}"));
    });

    // Start child first, then parent
    engine.eval_lua(r#"Script.run("child")"#).await.unwrap();
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
    assert!(engine.is_running("child"));

    engine.eval_lua(r#"Script.run("parent")"#).await.unwrap();
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

    // Parent exited, child should be killed via die_with_me
    assert!(!engine.is_running("parent"));
    assert!(!engine.is_running("child"), "child should have been killed when parent exited");

    let errs = errors.lock().unwrap();
    assert!(errs.is_empty(), "errors: {:?}", *errs);
}

#[tokio::test]
async fn test_send_to_script_injects_line() {
    let engine = ScriptEngine::new();
    let (tx, _rx) = tokio::sync::broadcast::channel::<Arc<Vec<u8>>>(64);
    engine.set_downstream_channel(tx.clone());
    engine.install_lua_api().unwrap();

    let errors: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let err_cap = errors.clone();
    engine.set_script_error_hook(move |name, err| {
        err_cap.lock().unwrap().push(format!("{name}: {err}"));
    });

    // Receiver script: waits for a message via get()
    let tmp_recv = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(tmp_recv.path(), r#"
        local line = get()
        assert(line == "hello from sender", "expected message, got: " .. tostring(line))
    "#).unwrap();

    // Sender script: sends message to receiver
    let tmp_send = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(tmp_send.path(), r#"
        pause(0.05)
        send_to_script("receiver", "hello from sender")
    "#).unwrap();

    engine.start_script("receiver", tmp_recv.path().to_str().unwrap(), vec![]).unwrap();
    tokio::time::sleep(tokio::time::Duration::from_millis(20)).await;
    engine.start_script("sender", tmp_send.path().to_str().unwrap(), vec![]).unwrap();
    tokio::time::sleep(tokio::time::Duration::from_millis(300)).await;

    let errs = errors.lock().unwrap();
    assert!(errs.is_empty(), "script errors: {:?}", *errs);
    assert!(!engine.is_running("receiver"), "receiver should have finished");
}

#[tokio::test]
async fn test_convenience_aliases() {
    use revenant::game_state::GameState;
    let gs = Arc::new(std::sync::RwLock::new(GameState::default()));
    {
        let mut state = gs.write().unwrap();
        state.health = 100;
        state.max_health = 200;
        state.mana = 50;
        state.stunned = true;
        state.room_name = "Town Square".to_string();
    }

    let engine = ScriptEngine::new();
    engine.set_game_state(gs);
    engine.install_lua_api().unwrap();

    engine.eval_lua(r#"
        assert(health() == 100, "health() failed")
        assert(max_health() == 200, "max_health() failed")
        assert(mana() == 50, "mana() failed")
        assert(stunned() == true, "stunned() failed")
        assert(room_name() == "Town Square", "room_name() failed")
    "#).await.unwrap();
}
