use crate::script_engine::ScriptEngine;
use mlua::prelude::*;

pub fn register(engine: &ScriptEngine) -> LuaResult<()> {
    let lua = &engine.lua;
    let t = lua.create_table()?;

    // Script.pause(name) — pause a running script
    let paused = engine.paused.clone();
    t.set("pause", lua.create_function(move |_, name: String| {
        paused.lock().unwrap().insert(name);
        Ok(())
    })?)?;

    // Script.unpause(name) — resume a paused script
    let paused = engine.paused.clone();
    t.set("unpause", lua.create_function(move |_, name: String| {
        paused.lock().unwrap().remove(&name);
        Ok(())
    })?)?;

    // Script.kill(name) — abort a running script
    let running = engine.running.clone();
    t.set("kill", lua.create_async_function(move |_, name: String| {
        let running = running.clone();
        async move {
            let h = running.lock().unwrap().remove(&name);
            if let Some(h) = h { h.abort(); }
            Ok(())
        }
    })?)?;

    // Script.list() → array table of running script names (excludes hidden scripts)
    let running = engine.running.clone();
    let hidden_list = engine.hidden.clone();
    t.set("list", lua.create_function(move |lua, ()| {
        let names: Vec<String> = {
            let r = running.lock().unwrap();
            let hidden_set = hidden_list.lock().unwrap();
            r.iter()
                .filter(|(n, h)| !h.is_finished() && !hidden_set.contains(*n))
                .map(|(n, _)| n.clone())
                .collect()
        };
        let out = lua.create_table()?;
        for (i, name) in names.iter().enumerate() {
            out.set(i + 1, name.as_str())?;
        }
        Ok(out)
    })?)?;

    // Script.hidden() → array table of running hidden script names
    let running_hidden = engine.running.clone();
    let hidden_hidden = engine.hidden.clone();
    t.set("hidden", lua.create_function(move |lua, ()| {
        let names: Vec<String> = {
            let r = running_hidden.lock().unwrap();
            let hidden_set = hidden_hidden.lock().unwrap();
            r.iter()
                .filter(|(n, h)| !h.is_finished() && hidden_set.contains(*n))
                .map(|(n, _)| n.clone())
                .collect()
        };
        let out = lua.create_table()?;
        for (i, name) in names.iter().enumerate() {
            out.set(i + 1, name.as_str())?;
        }
        Ok(out)
    })?)?;

    // Script.running(name) → bool — true if the script is running and not finished
    let running = engine.running.clone();
    t.set("running", lua.create_function(move |_, name: String| {
        Ok(running.lock().unwrap()
            .get(&name)
            .map(|h| !h.is_finished())
            .unwrap_or(false))
    })?)?;

    // Script.run(name [, args_string]) — launch a script by name from the scripts directory
    let running2 = engine.running.clone();
    let script_args2 = engine.script_args.clone();
    let lua_ref = engine.lua.clone();
    let scripts_dir2 = engine.scripts_dir.clone();
    let error_hook2 = engine.script_error_hook.clone();
    let thread_names_run = engine.thread_names.clone();
    let downstream_tx_run = engine.downstream_tx.clone();
    let script_lines_tx_run = engine.script_lines_tx.clone();
    let script_lines_rx_run = engine.script_lines_rx.clone();
    let at_exit_hooks_run = engine.at_exit_hooks.clone();
    let game_state_run = engine.game_state.clone();
    t.set("run", lua.create_async_function(move |_, (name, args_str): (String, Option<String>)| {
        let running2 = running2.clone();
        let script_args2 = script_args2.clone();
        let lua_ref = lua_ref.clone();
        let scripts_dir2 = scripts_dir2.clone();
        let error_hook2 = error_hook2.clone();
        let thread_names_run = thread_names_run.clone();
        let downstream_tx_run = downstream_tx_run.clone();
        let script_lines_tx_run = script_lines_tx_run.clone();
        let script_lines_rx_run = script_lines_rx_run.clone();
        let at_exit_hooks_run = at_exit_hooks_run.clone();
        let game_state_run = game_state_run.clone();
        async move {
            let dir = scripts_dir2.lock().unwrap().clone();

            // Determine game subdir for game-specific script resolution
            let game_sub = {
                let guard = game_state_run.lock().unwrap();
                match guard.as_ref() {
                    Some(gs) => match gs.read().unwrap_or_else(|e| e.into_inner()).game {
                        crate::game_state::Game::DragonRealms => "dr",
                        crate::game_state::Game::GemStone => "gs",
                    },
                    None => "gs",
                }
            };

            // Game-aware lookup: {game}/{name} → {name}
            let path = crate::dispatch::resolve_script_path(&dir, game_sub, &name)
                .ok_or_else(|| mlua::Error::RuntimeError(format!(
                    "script not found: {name} (checked {game_sub}/{name} and {name})"
                )))?;

            let args_string = args_str.unwrap_or_default();
            let mut args = if args_string.is_empty() { vec![] } else { vec![args_string.clone()] };
            args.extend(args_string.split_whitespace().map(|s| s.to_string()));

            // Store args
            script_args2.lock().unwrap().insert(name.clone(), args.clone());

            let raw_code = std::fs::read_to_string(&path)
                .map_err(|e| mlua::Error::RuntimeError(e.to_string()))?;

            // If this is a package script, wrap code with scoped package.path
            let code = if path.ends_with("/init.lua") {
                if let Some(pkg_dir) = std::path::Path::new(&path).parent() {
                    let pkg_dir_str = pkg_dir.to_string_lossy();
                    let wrapper = format!(
                        "do\nlocal _saved_path = package.path\npackage.path = \"{}/?.lua;{}/?.lua;\" .. package.path\nlocal _ok, _err = pcall(function()\n",
                        pkg_dir_str, dir
                    );
                    wrapper + &raw_code + "\nend)\npackage.path = _saved_path\nif not _ok then error(_err) end\nend"
                } else {
                    raw_code
                }
            } else {
                raw_code
            };

            // Set globals before launch
            {
                let globals = lua_ref.globals();
                globals.set("_REVENANT_SCRIPT", name.as_str())?;
                let args_table = lua_ref.create_table()?;
                for (i, a) in args.iter().enumerate() {
                    args_table.raw_set(i as i64, a.as_str())?;
                }
                let all: mlua::Table = globals.get("_REVENANT_SCRIPT_ARGS")
                    .unwrap_or_else(|_| lua_ref.create_table().unwrap());
                all.set(name.as_str(), args_table)?;
                globals.set("_REVENANT_SCRIPT_ARGS", all)?;
            }

            // Create per-script line buffer (MPSC channel)
            let (lines_tx, lines_rx) = tokio::sync::mpsc::unbounded_channel::<String>();
            script_lines_tx_run.lock().unwrap().insert(name.clone(), lines_tx);
            script_lines_rx_run.lock().unwrap().insert(
                name.clone(),
                std::sync::Arc::new(tokio::sync::Mutex::new(lines_rx)),
            );

            // Spawn feeder task: broadcast channel → per-script MPSC buffer
            if let Some(broadcast_tx) = downstream_tx_run.lock().unwrap().as_ref() {
                let mut broadcast_rx = broadcast_tx.subscribe();
                let feeder_tx = script_lines_tx_run.lock().unwrap().get(&name).unwrap().clone();
                let feeder_name = name.clone();
                tokio::spawn(async move {
                    loop {
                        match broadcast_rx.recv().await {
                            Ok(bytes) => {
                                let text = String::from_utf8_lossy(&bytes);
                                for line in text.lines() {
                                    let trimmed = line.trim_end();
                                    if !trimmed.is_empty() {
                                        if feeder_tx.send(trimmed.to_string()).is_err() {
                                            return; // receiver dropped, script exited
                                        }
                                    }
                                }
                            }
                            Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                            Err(tokio::sync::broadcast::error::RecvError::Closed) => return,
                        }
                    }
                });
                let _ = feeder_name; // suppress unused warning
            }

            let lua_clone = lua_ref.clone();
            let script_name = name.clone();
            let error_hook = error_hook2.clone();
            let thread_names = thread_names_run.clone();
            let script_lines_tx_clone = script_lines_tx_run.clone();
            let script_lines_rx_clone = script_lines_rx_run.clone();
            let at_exit_hooks_clone = at_exit_hooks_run.clone();
            let handle = tokio::spawn(async move {
                let result: LuaResult<()> = async {
                    let func: LuaFunction = lua_clone.load(&code).set_name(&script_name).into_function()?;
                    let thread = lua_clone.create_thread(func)?;
                    // Register per-coroutine identity: thread pointer → script name
                    let ptr = thread.to_pointer() as usize;
                    thread_names.lock().unwrap().insert(ptr, script_name.clone());
                    let r = thread.into_async::<mlua::MultiValue>(mlua::MultiValue::new()).await;
                    thread_names.lock().unwrap().remove(&ptr);
                    r?;
                    Ok(())
                }.await;
                // Run at-exit hooks (LIFO) — runs on both success and error
                let hooks = at_exit_hooks_clone.lock().unwrap().remove(&script_name);
                if let Some(hook_keys) = hooks {
                    for key in hook_keys.into_iter().rev() {
                        if let Ok(func) = lua_clone.registry_value::<LuaFunction>(&key) {
                            if let Err(e) = func.call_async::<()>(()).await {
                                tracing::warn!("[script:{script_name}] at_exit hook error: {e}");
                            }
                        }
                        let _ = lua_clone.remove_registry_value(key);
                    }
                }
                if let Err(e) = result {
                    let msg = e.to_string();
                    if !msg.contains("[script exit]") {
                        tracing::error!("[script:{script_name}] error: {msg}");
                        if let Some(hook) = error_hook.lock().unwrap().as_ref() {
                            hook(script_name.clone(), msg);
                        }
                    }
                }
                // Clean up args table to avoid unbounded growth
                if let Ok(globals) = lua_clone.globals().get::<mlua::Table>("_REVENANT_SCRIPT_ARGS") {
                    let _ = globals.raw_remove(script_name.as_str());
                }
                // Clean up line buffer entries
                script_lines_tx_clone.lock().unwrap().remove(&script_name);
                script_lines_rx_clone.lock().unwrap().remove(&script_name);
            });
            running2.lock().unwrap().insert(name, handle);
            Ok(())
        }
    })?)?;

    // Script.pause_all() — pause all running scripts except the caller
    {
        let paused = engine.paused.clone();
        let running = engine.running.clone();
        let no_pause = engine.no_pause_all.clone();
        let thread_names = engine.thread_names.clone();
        t.set("pause_all", lua.create_function(move |lua, ()| {
            let ptr = lua.current_thread().to_pointer() as usize;
            let caller = thread_names.lock().unwrap().get(&ptr).cloned();
            let protected = no_pause.lock().unwrap().clone();
            let names: Vec<String> = running.lock().unwrap().iter()
                .filter(|(n, h)| !h.is_finished() && !protected.contains(*n) && caller.as_deref() != Some(n.as_str()))
                .map(|(n, _)| n.clone())
                .collect();
            let mut p = paused.lock().unwrap();
            for n in names { p.insert(n); }
            Ok(())
        })?)?;
    }

    // Script.unpause_all() — unpause all scripts
    {
        let paused = engine.paused.clone();
        t.set("unpause_all", lua.create_function(move |_, ()| {
            paused.lock().unwrap().clear();
            Ok(())
        })?)?;
    }

    // Script.exit() — cleanly exit the current script
    t.set("exit", lua.create_function(move |_, ()| -> LuaResult<()> {
        Err(LuaError::RuntimeError("[script exit]".into()))
    })?)?;

    // Script.exists(name) — check if a script file exists (game-aware)
    let scripts_dir_exists = engine.scripts_dir.clone();
    let game_state_exists = engine.game_state.clone();
    t.set("exists", lua.create_function(move |_, name: String| {
        let dir = scripts_dir_exists.lock().unwrap().clone();
        let game_sub = {
            let guard = game_state_exists.lock().unwrap();
            match guard.as_ref() {
                Some(gs) => match gs.read().unwrap_or_else(|e| e.into_inner()).game {
                    crate::game_state::Game::DragonRealms => "dr",
                    crate::game_state::Game::GemStone => "gs",
                },
                None => "gs",
            }
        };
        Ok(crate::dispatch::resolve_script_path(&dir, game_sub, &name).is_some())
    })?)?;

    let globals = lua.globals();

    // Script.at_exit — alias for before_dying (set on t after globals registration below)
    // Script.clear_exit_procs — alias for undo_before_dying

    // before_dying(func) — register at-exit callback for current script
    let at_exit_bd = engine.at_exit_hooks.clone();
    let thread_names_bd = engine.thread_names.clone();
    globals.set("before_dying", lua.create_function(move |lua, func: LuaFunction| {
        let thread = lua.current_thread();
        let ptr = thread.to_pointer() as usize;
        let script_name: String = thread_names_bd.lock().unwrap()
            .get(&ptr).cloned()
            .ok_or_else(|| LuaError::RuntimeError("before_dying() called outside script context".into()))?;
        let key = lua.create_registry_value(func)?;
        at_exit_bd.lock().unwrap().entry(script_name).or_default().push(key);
        Ok(())
    })?)?;

    // Script.at_exit — alias for before_dying (set after t is fully built, below)

    // undo_before_dying() — clear all at-exit hooks for current script
    let at_exit_ubd = engine.at_exit_hooks.clone();
    let thread_names_ubd = engine.thread_names.clone();
    globals.set("undo_before_dying", lua.create_function(move |lua, ()| {
        let thread = lua.current_thread();
        let ptr = thread.to_pointer() as usize;
        let script_name: String = thread_names_ubd.lock().unwrap()
            .get(&ptr).cloned()
            .ok_or_else(|| LuaError::RuntimeError("undo_before_dying() called outside script context".into()))?;
        at_exit_ubd.lock().unwrap().remove(&script_name);
        Ok(())
    })?)?;

    // no_kill_all() — toggle kill protection for current script
    let no_kill = engine.no_kill_all.clone();
    let thread_names_nka = engine.thread_names.clone();
    globals.set("no_kill_all", lua.create_function(move |lua, ()| {
        let thread = lua.current_thread();
        let ptr = thread.to_pointer() as usize;
        let script_name: String = thread_names_nka.lock().unwrap()
            .get(&ptr).cloned()
            .ok_or_else(|| LuaError::RuntimeError("no_kill_all() called outside script context".into()))?;
        let mut set = no_kill.lock().unwrap();
        if set.contains(&script_name) {
            set.remove(&script_name);
        } else {
            set.insert(script_name);
        }
        Ok(())
    })?)?;

    // no_pause_all() — toggle pause protection for current script
    let no_pause = engine.no_pause_all.clone();
    let thread_names_npa = engine.thread_names.clone();
    globals.set("no_pause_all", lua.create_function(move |lua, ()| {
        let thread = lua.current_thread();
        let ptr = thread.to_pointer() as usize;
        let script_name: String = thread_names_npa.lock().unwrap()
            .get(&ptr).cloned()
            .ok_or_else(|| LuaError::RuntimeError("no_pause_all() called outside script context".into()))?;
        let mut set = no_pause.lock().unwrap();
        if set.contains(&script_name) {
            set.remove(&script_name);
        } else {
            set.insert(script_name);
        }
        Ok(())
    })?)?;

    // running(name) — global alias for Script.running(name)
    let running_alias = engine.running.clone();
    globals.set("running", lua.create_function(move |_, name: String| {
        Ok(running_alias.lock().unwrap()
            .get(&name)
            .map(|h| !h.is_finished())
            .unwrap_or(false))
    })?)?;

    // Build a metatable for the Script table so that Script.vars and Script.name
    // are computed properties (not stored values). The __index metamethod intercepts
    // field access and returns the current value from the per-thread identity map.
    let thread_names_meta = engine.thread_names.clone();
    let mt = lua.create_table()?;
    mt.set("__index", lua.create_function(move |lua, (_tbl, key): (LuaValue, String)| {
        let ptr = lua.current_thread().to_pointer() as usize;
        let script_name: String = thread_names_meta.lock().unwrap()
            .get(&ptr).cloned()
            .unwrap_or_else(|| lua.globals().get("_REVENANT_SCRIPT").unwrap_or_default());
        match key.as_str() {
            "vars" => {
                // Return the args table for the currently running script
                let all_args: mlua::Result<mlua::Table> = lua.globals().get("_REVENANT_SCRIPT_ARGS");
                match all_args {
                    Ok(t) => match t.get::<mlua::Table>(script_name.as_str()) {
                        Ok(v) => Ok(mlua::Value::Table(v)),
                        Err(_) => Ok(mlua::Value::Nil),
                    },
                    Err(_) => Ok(mlua::Value::Nil),
                }
            }
            "name" => {
                // Return the name of the currently running script
                Ok(mlua::Value::String(lua.create_string(&script_name)?))
            }
            _ => Ok(mlua::Value::Nil),
        }
    })?)?;
    t.set_metatable(Some(mt));

    // Script.at_exit = before_dying alias
    let before_dying_fn: LuaFunction = lua.globals().get("before_dying")?;
    t.set("at_exit", before_dying_fn)?;

    // Script.clear_exit_procs = undo_before_dying alias
    let undo_fn: LuaFunction = lua.globals().get("undo_before_dying")?;
    t.set("clear_exit_procs", undo_fn)?;

    lua.globals().set("Script", t)?;
    Ok(())
}
