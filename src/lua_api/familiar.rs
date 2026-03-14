use crate::script_engine::ScriptEngine;
use mlua::prelude::*;

pub fn register(engine: &ScriptEngine) -> LuaResult<()> {
    let gs_arc = match engine.game_state.lock().unwrap().clone() {
        Some(gs) => gs,
        None => return Ok(()),
    };

    let lua = &engine.lua;
    let familiar = lua.create_table()?;

    let gs = gs_arc.clone();
    let mt = lua.create_table()?;
    mt.set("__index", lua.create_function(move |lua, (_t, key): (LuaTable, String)| {
        let gs = gs.read().unwrap();
        match key.as_str() {
            "room_title" => Ok(LuaValue::String(lua.create_string(&gs.familiar_room_title)?)),
            "room_description" => Ok(LuaValue::String(lua.create_string(&gs.familiar_room_description)?)),
            "room_exits" => {
                let t = lua.create_table()?;
                for (i, exit) in gs.familiar_room_exits.iter().enumerate() {
                    t.raw_set(i + 1, exit.as_str())?;
                }
                Ok(LuaValue::Table(t))
            }
            _ => Ok(LuaValue::Nil),
        }
    })?)?;
    familiar.set_metatable(Some(mt));

    lua.globals().set("Familiar", familiar)?;
    Ok(())
}
