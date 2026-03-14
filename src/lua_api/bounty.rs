use crate::script_engine::ScriptEngine;
use mlua::prelude::*;

pub fn register(engine: &ScriptEngine) -> LuaResult<()> {
    let gs_arc = match engine.game_state.lock().unwrap().clone() {
        Some(gs) => gs,
        None => return Ok(()),
    };

    let lua = &engine.lua;
    let bounty = lua.create_table()?;

    let gs = gs_arc.clone();
    let mt = lua.create_table()?;
    mt.set("__index", lua.create_function(move |lua, (_t, key): (LuaTable, String)| {
        let gs = gs.read().unwrap();
        match key.as_str() {
            "task" => Ok(LuaValue::String(lua.create_string(&gs.bounty_task)?)),
            _ => Ok(LuaValue::Nil),
        }
    })?)?;
    bounty.set_metatable(Some(mt));

    lua.globals().set("Bounty", bounty)?;
    Ok(())
}
