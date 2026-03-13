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

    // respond(text) — echo to client only (v1: stdout stub)
    globals.set("respond", lua.create_function(|_, text: String| {
        println!("[respond] {text}");
        Ok(())
    })?)?;

    // pause(seconds) — async sleep
    globals.set("pause", lua.create_async_function(|_, secs: f64| async move {
        if secs > 0.0 {
            tokio::time::sleep(tokio::time::Duration::from_secs_f64(secs)).await;
        }
        Ok(())
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

    // fput(cmd) — put + wait for prompt (v1: just put, prompt wait is TODO)
    let sink = engine.upstream_sink.clone();
    globals.set("fput", lua.create_async_function(move |_, cmd: String| {
        let sink = sink.clone();
        async move {
            let line = if cmd.ends_with('\n') { cmd } else { format!("{cmd}\n") };
            if let Some(f) = sink.lock().unwrap().as_ref() { f(line); }
            // TODO: wait for next <prompt> event via downstream channel.
            // IMPORTANT: ensure the sink lock above is fully dropped before adding any
            // .await here — holding a std::sync::Mutex guard across an await point will
            // deadlock on single-threaded tokio executors.
            Ok(())
        }
    })?)?;

    Ok(())
}
