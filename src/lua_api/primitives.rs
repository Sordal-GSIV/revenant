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

    // respond(text) — echo to client TCP stream (also logged to respond_log for monitor)
    // When safe_to_respond is false (DR in-stream), buffer output and flush later.
    let engine_respond = engine.respond_sink.clone();
    let respond_log = engine.respond_log.clone();
    let safe_flag = engine.safe_to_respond.clone();
    globals.set("respond", lua.create_function(move |_, text: String| {
        {
            let mut log = respond_log.lock().unwrap();
            if log.len() >= 500 { log.pop_front(); }
            log.push_back(text.clone());
        }
        // DR safety: wait briefly if not safe to inject output into stream
        if !safe_flag.load(std::sync::atomic::Ordering::Relaxed) {
            // Spin up to 500ms waiting for safe window
            for _ in 0..50 {
                std::thread::sleep(std::time::Duration::from_millis(10));
                if safe_flag.load(std::sync::atomic::Ordering::Relaxed) { break; }
            }
        }
        if let Some(f) = engine_respond.lock().unwrap().as_ref() {
            f(format!("<output class=\"mono\">{text}</output>\n"));
        } else {
            println!("[respond] {text}");
        }
        Ok(())
    })?)?;

    // pause(seconds) — async sleep, pause-aware
    let paused = engine.paused.clone();
    let thread_names_pause = engine.thread_names.clone();
    globals.set("pause", lua.create_async_function(move |lua, secs: f64| {
        let paused = paused.clone();
        let thread_names = thread_names_pause.clone();
        async move {
            let ptr = lua.current_thread().to_pointer() as usize;
            let script_name: String = thread_names.lock().unwrap()
                .get(&ptr).cloned()
                .unwrap_or_else(|| lua.globals().get("_REVENANT_SCRIPT").unwrap_or_default());

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

    // waitrt() — sleep until roundtime expires
    let gs_waitrt = engine.game_state.clone();
    globals.set("waitrt", lua.create_async_function(move |_, ()| {
        let gs_waitrt = gs_waitrt.clone();
        async move {
            let rt = {
                let lock = gs_waitrt.lock().unwrap();
                match lock.as_ref() {
                    Some(gs) => gs.read().unwrap().roundtime(),
                    None => 0.0,
                }
            };
            if rt > 0.0 {
                tokio::time::sleep(tokio::time::Duration::from_secs_f64(rt + 0.1)).await;
            }
            Ok(())
        }
    })?)?;

    // waitcastrt() — sleep until cast roundtime expires
    let gs_waitcastrt = engine.game_state.clone();
    globals.set("waitcastrt", lua.create_async_function(move |_, ()| {
        let gs_waitcastrt = gs_waitcastrt.clone();
        async move {
            let rt = {
                let lock = gs_waitcastrt.lock().unwrap();
                match lock.as_ref() {
                    Some(gs) => gs.read().unwrap().cast_roundtime(),
                    None => 0.0,
                }
            };
            if rt > 0.0 {
                tokio::time::sleep(tokio::time::Duration::from_secs_f64(rt + 0.1)).await;
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

    // get() — block until next downstream line for this script
    // If the script is in unique mode, reads from unique buffer instead.
    let script_lines_rx_get = engine.script_lines_rx.clone();
    let thread_names_get = engine.thread_names.clone();
    let want_uniq_get = engine.want_unique.clone();
    let uniq_rx_get = engine.unique_lines_rx.clone();
    globals.set("get", lua.create_async_function(move |lua, ()| {
        let script_lines_rx = script_lines_rx_get.clone();
        let thread_names = thread_names_get.clone();
        let want_uniq = want_uniq_get.clone();
        let uniq_rx = uniq_rx_get.clone();
        async move {
            let ptr = lua.current_thread().to_pointer() as usize;
            let script_name: String = {
                let map = thread_names.lock().unwrap();
                map.get(&ptr).cloned().ok_or_else(|| LuaError::RuntimeError("get() called outside script context".into()))?
            };
            let is_unique = want_uniq.lock().unwrap().contains(&script_name);
            let rx = if is_unique {
                let map = uniq_rx.lock().unwrap();
                map.get(&script_name).cloned()
                    .ok_or_else(|| LuaError::RuntimeError("unique buffer not initialized".into()))?
            } else {
                let map = script_lines_rx.lock().unwrap();
                map.get(&script_name).cloned()
                    .ok_or_else(|| LuaError::RuntimeError(format!("no line buffer for script {script_name}")))?
            };
            let mut rx_guard = rx.lock().await;
            match rx_guard.recv().await {
                Some(line) => Ok(line),
                None => Err(LuaError::RuntimeError("line buffer closed".into())),
            }
        }
    })?)?;

    // echo(msg) — respond with script name prefix
    let respond_sink = engine.respond_sink.clone();
    let respond_log = engine.respond_log.clone();
    let thread_names_echo = engine.thread_names.clone();
    let no_echo_echo = engine.no_echo.clone();
    globals.set("echo", lua.create_function(move |lua, msg: String| {
        let thread = lua.current_thread();
        let ptr = thread.to_pointer() as usize;
        let script_name: String = thread_names_echo.lock().unwrap()
            .get(&ptr).cloned()
            .unwrap_or_else(|| lua.globals().get("_REVENANT_SCRIPT").unwrap_or_else(|_| "unknown".to_string()));
        if no_echo_echo.lock().unwrap().contains(&script_name) {
            return Ok(());
        }
        let text = format!("[{script_name}]: {msg}");
        {
            let mut log = respond_log.lock().unwrap();
            if log.len() >= 500 { log.pop_front(); }
            log.push_back(text.clone());
        }
        if let Some(f) = respond_sink.lock().unwrap().as_ref() {
            f(format!("<output class=\"mono\">{text}</output>\n"));
        } else {
            println!("[echo] {text}");
        }
        Ok(())
    })?)?;

    // get_noblock() / nget() — non-blocking variant of get()
    let script_lines_rx2 = engine.script_lines_rx.clone();
    let thread_names2 = engine.thread_names.clone();
    let get_noblock_fn = lua.create_function(move |lua, ()| {
        let ptr = lua.current_thread().to_pointer() as usize;
        let script_name: String = {
            let map = thread_names2.lock().unwrap();
            map.get(&ptr).cloned().ok_or_else(|| LuaError::RuntimeError("get_noblock() called outside script context".into()))?
        };
        let rx = {
            let map = script_lines_rx2.lock().unwrap();
            map.get(&script_name).cloned()
                .ok_or_else(|| LuaError::RuntimeError(format!("no line buffer for script {script_name}")))?
        };
        let result = match rx.try_lock() {
            Ok(mut guard) => match guard.try_recv() {
                Ok(line) => Ok(mlua::Value::String(lua.create_string(&line)?)),
                Err(tokio::sync::mpsc::error::TryRecvError::Empty) => Ok(mlua::Value::Nil),
                Err(tokio::sync::mpsc::error::TryRecvError::Disconnected) =>
                    Err(LuaError::RuntimeError("line buffer closed".into())),
            },
            Err(_) => Ok(mlua::Value::Nil), // receiver locked by async get()
        };
        result
    })?;
    globals.set("get_noblock", get_noblock_fn.clone())?;
    globals.set("nget", get_noblock_fn)?;

    // clear() — drain the script's line buffer, return all pending lines
    let script_lines_rx3 = engine.script_lines_rx.clone();
    let thread_names_clear = engine.thread_names.clone();
    globals.set("clear", lua.create_function(move |lua, ()| {
        let thread = lua.current_thread();
        let ptr = thread.to_pointer() as usize;
        let script_name: String = thread_names_clear.lock().unwrap()
            .get(&ptr).cloned()
            .ok_or_else(|| LuaError::RuntimeError("clear() called outside script context".into()))?;
        let rx = {
            let map = script_lines_rx3.lock().unwrap();
            map.get(&script_name).cloned()
                .ok_or_else(|| LuaError::RuntimeError(format!("no line buffer for script {script_name}")))?
        };
        let table = lua.create_table()?;
        let mut i = 1;
        if let Ok(mut guard) = rx.try_lock() {
            while let Ok(line) = guard.try_recv() {
                table.set(i, line.as_str())?;
                i += 1;
            }
        }
        Ok(table)
    })?)?;

    // reget(n) — return last N lines from game_log
    let game_log = engine.game_log.clone();
    globals.set("reget", lua.create_function(move |lua, n: usize| {
        let log = game_log.lock().unwrap();
        let len = log.len();
        let start = if n >= len { 0 } else { len - n };
        let table = lua.create_table()?;
        for (i, line) in log.iter().skip(start).enumerate() {
            table.set(i + 1, line.as_str())?;
        }
        Ok(table)
    })?)?;

    // send_to_script(name, msg) — inject a line into another script's line buffer
    // Routes to unique buffer if target script is in unique mode.
    let script_lines_tx = engine.script_lines_tx.clone();
    let want_uniq_sts = engine.want_unique.clone();
    let uniq_tx_sts = engine.unique_lines_tx.clone();
    globals.set("send_to_script", lua.create_function(move |_, (name, msg): (String, String)| {
        let is_unique = want_uniq_sts.lock().unwrap().contains(&name);
        if is_unique {
            if let Some(tx) = uniq_tx_sts.lock().unwrap().get(&name) {
                let _ = tx.send(msg);
            }
        } else {
            let map = script_lines_tx.lock().unwrap();
            if let Some(tx) = map.get(&name) {
                let _ = tx.send(msg); // silently drop if target script exited
            }
        }
        Ok(())
    })?)?;

    // hide_me() / silence_me() — toggle current script in hidden set
    let hidden_hm = engine.hidden.clone();
    let thread_names_hm = engine.thread_names.clone();
    let hide_me_fn = lua.create_function(move |lua, ()| {
        let ptr = lua.current_thread().to_pointer() as usize;
        let script_name: String = thread_names_hm.lock().unwrap()
            .get(&ptr).cloned()
            .ok_or_else(|| LuaError::RuntimeError("hide_me() called outside script context".into()))?;
        let mut set = hidden_hm.lock().unwrap();
        if set.contains(&script_name) { set.remove(&script_name); } else { set.insert(script_name); }
        Ok(())
    })?;
    globals.set("hide_me", hide_me_fn.clone())?;
    globals.set("silence_me", hide_me_fn)?;

    // toggle_echo() — toggle no_echo for current script
    let no_echo_te = engine.no_echo.clone();
    let thread_names_te = engine.thread_names.clone();
    globals.set("toggle_echo", lua.create_function(move |lua, ()| {
        let ptr = lua.current_thread().to_pointer() as usize;
        let script_name: String = thread_names_te.lock().unwrap()
            .get(&ptr).cloned()
            .ok_or_else(|| LuaError::RuntimeError("toggle_echo() called outside script context".into()))?;
        let mut set = no_echo_te.lock().unwrap();
        if set.contains(&script_name) { set.remove(&script_name); } else { set.insert(script_name); }
        Ok(())
    })?)?;

    // echo_on() — remove current script from no_echo
    let no_echo_on = engine.no_echo.clone();
    let thread_names_on = engine.thread_names.clone();
    globals.set("echo_on", lua.create_function(move |lua, ()| {
        let ptr = lua.current_thread().to_pointer() as usize;
        let script_name: String = thread_names_on.lock().unwrap()
            .get(&ptr).cloned()
            .ok_or_else(|| LuaError::RuntimeError("echo_on() called outside script context".into()))?;
        no_echo_on.lock().unwrap().remove(&script_name);
        Ok(())
    })?)?;

    // echo_off() — add current script to no_echo
    let no_echo_off = engine.no_echo.clone();
    let thread_names_off = engine.thread_names.clone();
    globals.set("echo_off", lua.create_function(move |lua, ()| {
        let ptr = lua.current_thread().to_pointer() as usize;
        let script_name: String = thread_names_off.lock().unwrap()
            .get(&ptr).cloned()
            .ok_or_else(|| LuaError::RuntimeError("echo_off() called outside script context".into()))?;
        no_echo_off.lock().unwrap().insert(script_name);
        Ok(())
    })?)?;

    // _raw_fput(cmd) — put + wait for <prompt> from downstream (low-level; use fput() from Lua)
    let sink = engine.upstream_sink.clone();
    let dtx = engine.downstream_tx.clone();
    globals.set("_raw_fput", lua.create_async_function(move |_, cmd: String| {
        let sink = sink.clone();
        let dtx = dtx.clone();
        async move {
            // Subscribe to downstream FIRST to avoid missing a <prompt> that arrives
            // between send and subscribe (subscribe-before-send race fix).
            let mut rx = match dtx.lock().unwrap().as_ref() {
                Some(tx) => tx.subscribe(),
                None => return Ok(()),
            };
            // Send the command — lock dropped before .await
            let line = if cmd.ends_with('\n') { cmd } else { format!("{cmd}\n") };
            {
                let guard = sink.lock().unwrap();
                if let Some(f) = guard.as_ref() { f(line); }
            }
            let deadline = tokio::time::Instant::now() + tokio::time::Duration::from_secs(10);
            loop {
                let recv = tokio::time::timeout_at(deadline, rx.recv()).await;
                match recv {
                    Err(_) => break, // timed out
                    Ok(Err(tokio::sync::broadcast::error::RecvError::Lagged(_))) => continue, // fell behind, keep listening
                    Ok(Err(_)) => break, // Closed
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

    // toggle_upstream() — enable/disable upstream listening for current script
    let ubt = engine.upstream_broadcast_tx.clone();
    let want_up = engine.want_upstream.clone();
    let up_lines_tx = engine.upstream_lines_tx.clone();
    let up_lines_rx = engine.upstream_lines_rx.clone();
    let thread_names_tu = engine.thread_names.clone();
    globals.set("toggle_upstream", lua.create_function(move |lua, ()| {
        let ptr = lua.current_thread().to_pointer() as usize;
        let script_name: String = thread_names_tu.lock().unwrap()
            .get(&ptr).cloned()
            .ok_or_else(|| LuaError::RuntimeError("toggle_upstream() called outside script context".into()))?;

        let mut set = want_up.lock().unwrap();
        if set.contains(&script_name) {
            set.remove(&script_name);
            up_lines_tx.lock().unwrap().remove(&script_name);
            up_lines_rx.lock().unwrap().remove(&script_name);
        } else {
            set.insert(script_name.clone());
            let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<String>();
            up_lines_tx.lock().unwrap().insert(script_name.clone(), tx.clone());
            up_lines_rx.lock().unwrap().insert(
                script_name.clone(),
                std::sync::Arc::new(tokio::sync::Mutex::new(rx)),
            );
            // Spawn feeder: upstream broadcast → per-script MPSC
            if let Some(broadcast_tx) = ubt.lock().unwrap().as_ref() {
                let mut broadcast_rx = broadcast_tx.subscribe();
                let feeder_tx = tx;
                tokio::spawn(async move {
                    loop {
                        match broadcast_rx.recv().await {
                            Ok(bytes) => {
                                let text = String::from_utf8_lossy(&bytes);
                                for line in text.lines() {
                                    let trimmed = line.trim_end();
                                    if !trimmed.is_empty() {
                                        if feeder_tx.send(trimmed.to_string()).is_err() {
                                            return;
                                        }
                                    }
                                }
                            }
                            Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                            Err(tokio::sync::broadcast::error::RecvError::Closed) => return,
                        }
                    }
                });
            }
        }
        Ok(())
    })?)?;

    // upstream_get() — block until next upstream line for this script
    let up_lines_rx_get = engine.upstream_lines_rx.clone();
    let thread_names_ug = engine.thread_names.clone();
    globals.set("upstream_get", lua.create_async_function(move |lua, ()| {
        let up_lines_rx = up_lines_rx_get.clone();
        let thread_names = thread_names_ug.clone();
        async move {
            let ptr = lua.current_thread().to_pointer() as usize;
            let script_name: String = {
                let map = thread_names.lock().unwrap();
                map.get(&ptr).cloned()
                    .ok_or_else(|| LuaError::RuntimeError("upstream_get() called outside script context".into()))?
            };
            let rx = {
                let map = up_lines_rx.lock().unwrap();
                map.get(&script_name).cloned()
                    .ok_or_else(|| LuaError::RuntimeError("upstream not enabled — call toggle_upstream() first".into()))?
            };
            let mut rx_guard = rx.lock().await;
            match rx_guard.recv().await {
                Some(line) => Ok(line),
                None => Err(LuaError::RuntimeError("upstream buffer closed".into())),
            }
        }
    })?)?;

    // upstream_get_noblock() — non-blocking variant of upstream_get()
    let up_lines_rx_nb = engine.upstream_lines_rx.clone();
    let thread_names_unb = engine.thread_names.clone();
    globals.set("upstream_get_noblock", lua.create_function(move |lua, ()| {
        let ptr = lua.current_thread().to_pointer() as usize;
        let script_name: String = {
            let map = thread_names_unb.lock().unwrap();
            map.get(&ptr).cloned()
                .ok_or_else(|| LuaError::RuntimeError("upstream_get_noblock() called outside script context".into()))?
        };
        let rx = {
            let map = up_lines_rx_nb.lock().unwrap();
            map.get(&script_name).cloned()
                .ok_or_else(|| LuaError::RuntimeError("upstream not enabled — call toggle_upstream() first".into()))?
        };
        let result = match rx.try_lock() {
            Ok(mut guard) => match guard.try_recv() {
                Ok(line) => Ok(mlua::Value::String(lua.create_string(&line)?)),
                Err(tokio::sync::mpsc::error::TryRecvError::Empty) => Ok(mlua::Value::Nil),
                Err(tokio::sync::mpsc::error::TryRecvError::Disconnected) =>
                    Err(LuaError::RuntimeError("upstream buffer closed".into())),
            },
            Err(_) => Ok(mlua::Value::Nil),
        };
        result
    })?)?;

    // upstream_waitfor(pattern [, timeout_secs]) — block until pattern appears in upstream
    let ubt_wf = engine.upstream_broadcast_tx.clone();
    globals.set("upstream_waitfor", lua.create_async_function(move |_, (pattern, timeout): (String, Option<f64>)| {
        let ubt = ubt_wf.clone();
        async move {
            let mut rx = match ubt.lock().unwrap().as_ref() {
                Some(tx) => tx.subscribe(),
                None => return Ok(()),
            };
            let deadline = timeout.map(|t| {
                tokio::time::Instant::now() + tokio::time::Duration::from_secs_f64(t)
            });
            loop {
                let recv = rx.recv();
                let bytes = match deadline {
                    Some(d) => match tokio::time::timeout_at(d, recv).await {
                        Ok(result) => result,
                        Err(_) => return Ok(()),
                    },
                    None => recv.await,
                };
                match bytes {
                    Ok(b) => {
                        let text = String::from_utf8_lossy(&b);
                        if text.contains(pattern.as_str()) {
                            return Ok(());
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                        return Err(LuaError::RuntimeError("upstream channel closed".into()));
                    }
                }
            }
        }
    })?)?;

    // i_stand_alone() / toggle_unique() — toggle unique buffer mode for current script
    let want_uniq = engine.want_unique.clone();
    let uniq_tx = engine.unique_lines_tx.clone();
    let uniq_rx = engine.unique_lines_rx.clone();
    let thread_names_isa = engine.thread_names.clone();
    let isa_fn = lua.create_function(move |lua, ()| {
        let ptr = lua.current_thread().to_pointer() as usize;
        let script_name: String = thread_names_isa.lock().unwrap()
            .get(&ptr).cloned()
            .ok_or_else(|| LuaError::RuntimeError("i_stand_alone() called outside script context".into()))?;
        let mut set = want_uniq.lock().unwrap();
        if set.contains(&script_name) {
            set.remove(&script_name);
            uniq_tx.lock().unwrap().remove(&script_name);
            uniq_rx.lock().unwrap().remove(&script_name);
        } else {
            set.insert(script_name.clone());
            let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<String>();
            uniq_tx.lock().unwrap().insert(script_name.clone(), tx);
            uniq_rx.lock().unwrap().insert(
                script_name,
                std::sync::Arc::new(tokio::sync::Mutex::new(rx)),
            );
        }
        Ok(())
    })?;
    globals.set("i_stand_alone", isa_fn.clone())?;
    globals.set("toggle_unique", isa_fn)?;

    // unique_get() — blocking read from unique buffer (always reads unique, regardless of mode)
    let uniq_rx_ug = engine.unique_lines_rx.clone();
    let thread_names_ug2 = engine.thread_names.clone();
    globals.set("unique_get", lua.create_async_function(move |lua, ()| {
        let uniq_rx = uniq_rx_ug.clone();
        let thread_names = thread_names_ug2.clone();
        async move {
            let ptr = lua.current_thread().to_pointer() as usize;
            let script_name: String = {
                let map = thread_names.lock().unwrap();
                map.get(&ptr).cloned()
                    .ok_or_else(|| LuaError::RuntimeError("unique_get() called outside script context".into()))?
            };
            let rx = {
                let map = uniq_rx.lock().unwrap();
                map.get(&script_name).cloned()
                    .ok_or_else(|| LuaError::RuntimeError("unique buffer not initialized — call i_stand_alone() first".into()))?
            };
            let mut rx_guard = rx.lock().await;
            match rx_guard.recv().await {
                Some(line) => Ok(line),
                None => Err(LuaError::RuntimeError("unique buffer closed".into())),
            }
        }
    })?)?;

    // unique_get_noblock() — non-blocking variant of unique_get()
    let uniq_rx_nb = engine.unique_lines_rx.clone();
    let thread_names_unb2 = engine.thread_names.clone();
    globals.set("unique_get_noblock", lua.create_function(move |lua, ()| {
        let ptr = lua.current_thread().to_pointer() as usize;
        let script_name: String = {
            let map = thread_names_unb2.lock().unwrap();
            map.get(&ptr).cloned()
                .ok_or_else(|| LuaError::RuntimeError("unique_get_noblock() called outside script context".into()))?
        };
        let rx = {
            let map = uniq_rx_nb.lock().unwrap();
            map.get(&script_name).cloned()
                .ok_or_else(|| LuaError::RuntimeError("unique buffer not initialized — call i_stand_alone() first".into()))?
        };
        let result = match rx.try_lock() {
            Ok(mut guard) => match guard.try_recv() {
                Ok(line) => Ok(mlua::Value::String(lua.create_string(&line)?)),
                Err(tokio::sync::mpsc::error::TryRecvError::Empty) => Ok(mlua::Value::Nil),
                Err(tokio::sync::mpsc::error::TryRecvError::Disconnected) =>
                    Err(LuaError::RuntimeError("unique buffer closed".into())),
            },
            Err(_) => Ok(mlua::Value::Nil),
        };
        result
    })?)?;

    // idle_p([secs]) — returns true if no upstream command has been forwarded for >= secs (default 60)
    let last_up = engine.last_upstream_time.clone();
    globals.set("idle_p", lua.create_function(move |_, secs: Option<f64>| {
        let threshold = secs.unwrap_or(60.0);
        let elapsed = last_up.lock().unwrap().elapsed().as_secs_f64();
        Ok(elapsed >= threshold)
    })?)?;

    // unique_waitfor(pattern [, timeout_secs]) — wait for pattern in unique buffer
    let uniq_rx_wf = engine.unique_lines_rx.clone();
    let thread_names_uwf = engine.thread_names.clone();
    globals.set("unique_waitfor", lua.create_async_function(move |lua, (pattern, timeout): (String, Option<f64>)| {
        let uniq_rx = uniq_rx_wf.clone();
        let thread_names = thread_names_uwf.clone();
        async move {
            let ptr = lua.current_thread().to_pointer() as usize;
            let script_name: String = {
                let map = thread_names.lock().unwrap();
                map.get(&ptr).cloned()
                    .ok_or_else(|| LuaError::RuntimeError("unique_waitfor() called outside script context".into()))?
            };
            let rx = {
                let map = uniq_rx.lock().unwrap();
                map.get(&script_name).cloned()
                    .ok_or_else(|| LuaError::RuntimeError("unique buffer not initialized — call i_stand_alone() first".into()))?
            };
            let deadline = timeout.map(|t| {
                tokio::time::Instant::now() + tokio::time::Duration::from_secs_f64(t)
            });
            let mut rx_guard = rx.lock().await;
            loop {
                let recv = rx_guard.recv();
                let line = match deadline {
                    Some(d) => match tokio::time::timeout_at(d, recv).await {
                        Ok(Some(l)) => l,
                        Ok(None) => return Err(LuaError::RuntimeError("unique buffer closed".into())),
                        Err(_) => return Ok(mlua::Value::Nil), // timed out
                    },
                    None => match recv.await {
                        Some(l) => l,
                        None => return Err(LuaError::RuntimeError("unique buffer closed".into())),
                    },
                };
                if line.contains(pattern.as_str()) {
                    return Ok(mlua::Value::String(lua.create_string(&line)?));
                }
            }
        }
    })?)?;

    Ok(())
}
