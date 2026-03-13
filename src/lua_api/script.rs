use crate::script_engine::ScriptEngine;
use mlua::prelude::*;

pub fn register(engine: &ScriptEngine) -> LuaResult<()> {
    let lua = &engine.lua;
    let t = lua.create_table()?;

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

    // Script.args — empty string by default; per-script args set before launch
    // In v1, args are passed as a string set before starting the script
    t.set("args", "")?;

    lua.globals().set("Script", t)?;
    Ok(())
}
