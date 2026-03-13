use revenant::script_engine::ScriptEngine;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

#[tokio::test]
async fn test_pause_halts_script_progress() {
    let e = Arc::new(ScriptEngine::new());
    e.set_upstream_sink(|_| {});
    e.install_lua_api().unwrap();

    let tmp = tempfile::NamedTempFile::with_suffix(".lua").unwrap();
    std::fs::write(tmp.path(), r#"
        for i = 1, 100 do
            pause(0.02)
            put("tick " .. i)
        end
    "#).unwrap();

    let ticks = Arc::new(AtomicU32::new(0));
    let ticks2 = ticks.clone();
    e.set_upstream_sink(move |line| {
        if line.starts_with("tick ") { ticks2.fetch_add(1, Ordering::SeqCst); }
    });

    e.start_script("looper", tmp.path().to_str().unwrap(), vec![]).unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    let before_pause = ticks.load(Ordering::SeqCst);
    assert!(before_pause > 0, "script should have ticked before pause");

    e.pause_script("looper");
    let snapshot = ticks.load(Ordering::SeqCst);
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    let after_pause = ticks.load(Ordering::SeqCst);
    assert!(after_pause <= snapshot + 1, "script should not advance while paused");

    e.unpause_script("looper");
    tokio::time::sleep(std::time::Duration::from_millis(150)).await;
    let after_unpause = ticks.load(Ordering::SeqCst);
    assert!(after_unpause > after_pause, "script should resume after unpause");

    e.kill_script("looper").await;
}
