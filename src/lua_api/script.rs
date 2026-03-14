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

    // Script.list() → array table of running script names
    let running = engine.running.clone();
    t.set("list", lua.create_function(move |lua, ()| {
        let names: Vec<String> = {
            let r = running.lock().unwrap();
            r.iter()
                .filter(|(_, h)| !h.is_finished())
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
    t.set("run", lua.create_async_function(move |_, (name, args_str): (String, Option<String>)| {
        let running2 = running2.clone();
        let script_args2 = script_args2.clone();
        let lua_ref = lua_ref.clone();
        let scripts_dir2 = scripts_dir2.clone();
        let error_hook2 = error_hook2.clone();
        async move {
            let dir = scripts_dir2.lock().unwrap().clone();

            // Two-step lookup: directory package first, then single file
            let pkg_init = format!("{dir}/{name}/init.lua");
            let single_file = format!("{dir}/{name}.lua");

            let path = if std::path::Path::new(&pkg_init).exists() {
                pkg_init
            } else if std::path::Path::new(&single_file).exists() {
                single_file
            } else {
                return Err(mlua::Error::RuntimeError(format!(
                    "script not found: {name} (checked {name}/init.lua and {name}.lua)"
                )));
            };

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

            let lua_clone = lua_ref.clone();
            let script_name = name.clone();
            let error_hook = error_hook2.clone();
            let handle = tokio::spawn(async move {
                let result: LuaResult<()> = async {
                    let func: LuaFunction = lua_clone.load(&code).set_name(&script_name).into_function()?;
                    let thread = lua_clone.create_thread(func)?;
                    thread.into_async::<mlua::MultiValue>(mlua::MultiValue::new()).await?;
                    Ok(())
                }.await;
                if let Err(e) = result {
                    let msg = e.to_string();
                    tracing::error!("[script:{script_name}] error: {msg}");
                    if let Some(hook) = error_hook.lock().unwrap().as_ref() {
                        hook(script_name.clone(), msg);
                    }
                }
                // Clean up args table to avoid unbounded growth
                if let Ok(globals) = lua_clone.globals().get::<mlua::Table>("_REVENANT_SCRIPT_ARGS") {
                    let _ = globals.raw_remove(script_name.as_str());
                }
            });
            running2.lock().unwrap().insert(name, handle);
            Ok(())
        }
    })?)?;

    // Build a metatable for the Script table so that Script.vars and Script.name
    // are computed properties (not stored values). The __index metamethod intercepts
    // field access and returns the current value from Lua globals.
    let mt = lua.create_table()?;
    mt.set("__index", lua.create_function(|lua, (_tbl, key): (LuaValue, String)| {
        match key.as_str() {
            "vars" => {
                // Return the args table for the currently running script
                let script_name: String = lua.globals().get("_REVENANT_SCRIPT")
                    .unwrap_or_else(|_| String::new());
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
                let name: String = lua.globals().get("_REVENANT_SCRIPT")
                    .unwrap_or_else(|_| String::new());
                Ok(mlua::Value::String(lua.create_string(&name)?))
            }
            _ => Ok(mlua::Value::Nil),
        }
    })?)?;
    t.set_metatable(Some(mt));

    lua.globals().set("Script", t)?;
    Ok(())
}
