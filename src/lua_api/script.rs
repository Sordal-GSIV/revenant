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
        let r = running.lock().unwrap();
        let out = lua.create_table()?;
        for (i, (name, handle)) in r.iter().enumerate() {
            if !handle.is_finished() {
                out.set(i + 1, name.as_str())?;
            }
        }
        Ok(out)
    })?)?;

    // Script.args — empty string by default; per-script args set before launch
    // In v1, args are passed as a string set before starting the script
    t.set("args", "")?;

    lua.globals().set("Script", t)?;
    Ok(())
}
