use crate::script_engine::ScriptEngine;
use mlua::prelude::*;

pub fn register(engine: &ScriptEngine) -> LuaResult<()> {
    let gs_arc = match engine.game_state.lock().unwrap().clone() {
        Some(gs) => gs,
        None => return Ok(()),
    };

    let lua = &engine.lua;
    let group = lua.create_table()?;

    // Group.members and Group.leader are managed by Lua-side group.lua
    group.set("members", lua.create_table()?)?;
    group.raw_set("leader", LuaValue::Nil)?;

    let gs = gs_arc.clone();
    let mt = lua.create_table()?;
    mt.set("__index", lua.create_function(move |_, (_t, key): (LuaTable, String)| {
        let gs = gs.read().unwrap();
        match key.as_str() {
            "joined" => Ok(LuaValue::Boolean(gs.joined)),
            _ => Ok(LuaValue::Nil),
        }
    })?)?;
    group.set_metatable(Some(mt));

    lua.globals().set("Group", group)?;
    Ok(())
}
