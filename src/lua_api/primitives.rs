use crate::script_engine::ScriptEngine;
use mlua::prelude::*;

pub fn register(engine: &ScriptEngine) -> LuaResult<()> {
    let lua = &engine.lua;
    let globals = lua.globals();

    // put(cmd) — send command to game server
    let sink = engine.upstream_sink.clone();
    globals.set("put", lua.create_function(move |_, cmd: String| {
        let line = if cmd.ends_with('\n') { cmd } else { format!("{cmd}\n") };
        if let Some(f) = sink.lock().unwrap().as_ref() { f(line); }
        Ok(())
    })?)?;

    // respond(text) — echo to client TCP stream
    let respond_sink = engine.respond_sink.clone();
    globals.set("respond", lua.create_function(move |_, text: String| {
        if let Some(f) = respond_sink.lock().unwrap().as_ref() {
            f(format!("<output class=\"mono\">{text}</output>\n"));
        } else {
            println!("[respond] {text}");
        }
        Ok(())
    })?)?;

    // pause(seconds) — async sleep, pause-aware
    let paused = engine.paused.clone();
    globals.set("pause", lua.create_async_function(move |lua, secs: f64| {
        let paused = paused.clone();
        async move {
            // NOTE: _REVENANT_SCRIPT is a shared Lua global set on launch. In concurrent-script
            // scenarios, this may read the wrong name if another script launched between yields.
            // Race condition accepted for v2; scripts are typically launched sequentially in practice.
            let script_name: String = lua.globals()
                .get("_REVENANT_SCRIPT").unwrap_or_default();

            // If already paused: wait in 0.1s increments until unpaused
            loop {
                let is_paused = paused.lock().unwrap().contains(&script_name);
                if !is_paused { break; }
                tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
            }

            // Normal sleep — check pause every 50ms
            if secs > 0.0 {
                let mut deadline = tokio::time::Instant::now()
                    + tokio::time::Duration::from_secs_f64(secs);
                loop {
                    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
                    let is_paused = paused.lock().unwrap().contains(&script_name);
                    if is_paused {
                        let pause_start = tokio::time::Instant::now();
                        loop {
                            let still_paused = paused.lock().unwrap().contains(&script_name);
                            if !still_paused { break; }
                            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                        }
                        // Freeze: extend deadline by the time spent paused
                        deadline += tokio::time::Instant::now() - pause_start;
                    }
                    if tokio::time::Instant::now() >= deadline { break; }
                }
            }
            Ok(())
        }
    })?)?;

    // waitfor(pattern [, timeout_secs]) — block coroutine until pattern appears downstream
    let dtx = engine.downstream_tx.clone();
    globals.set("waitfor", lua.create_async_function(move |_, (pattern, timeout): (String, Option<f64>)| {
        let dtx = dtx.clone();
        async move {
            let mut rx = match dtx.lock().unwrap().as_ref() {
                Some(tx) => tx.subscribe(),
                None => return Ok(()), // no channel yet
            };
            let deadline = timeout.map(|t| {
                tokio::time::Instant::now() + tokio::time::Duration::from_secs_f64(t)
            });
            loop {
                let recv = rx.recv();
                let bytes = match deadline {
                    Some(d) => match tokio::time::timeout_at(d, recv).await {
                        Ok(result) => result,
                        Err(_) => return Ok(()), // timed out
                    },
                    None => recv.await,
                };
                match bytes {
                    Ok(b) => {
                        // Convert bytes to lossy string for pattern matching.
                        // Note: patterns that straddle a TCP packet boundary will not match
                        // because neither fragment is a complete UTF-8 sequence. Acceptable
                        // for v1 (game output is ASCII/SGE XML).
                        let text = String::from_utf8_lossy(&b);
                        if text.contains(pattern.as_str()) {
                            return Ok(());
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                        return Err(LuaError::RuntimeError("downstream channel closed".into()));
                    }
                }
            }
        }
    })?)?;

    // fput(cmd) — put + wait for <prompt> from downstream
    let sink = engine.upstream_sink.clone();
    let dtx = engine.downstream_tx.clone();
    globals.set("fput", lua.create_async_function(move |_, cmd: String| {
        let sink = sink.clone();
        let dtx = dtx.clone();
        async move {
            // Send the command — lock dropped before .await
            let line = if cmd.ends_with('\n') { cmd } else { format!("{cmd}\n") };
            {
                let guard = sink.lock().unwrap();
                if let Some(f) = guard.as_ref() { f(line); }
            }
            // Subscribe to downstream — lock dropped before .await
            let mut rx = match dtx.lock().unwrap().as_ref() {
                Some(tx) => tx.subscribe(),
                None => return Ok(()),
            };
            let deadline = tokio::time::Instant::now() + tokio::time::Duration::from_secs(10);
            loop {
                let recv = tokio::time::timeout_at(deadline, rx.recv()).await;
                match recv {
                    Err(_) => break, // timed out
                    Ok(Err(_)) => break, // channel error
                    Ok(Ok(bytes)) => {
                        let text = String::from_utf8_lossy(&bytes);
                        if text.contains("<prompt") {
                            break;
                        }
                    }
                }
            }
            Ok(())
        }
    })?)?;

    Ok(())
}
