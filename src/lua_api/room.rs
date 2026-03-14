use crate::script_engine::ScriptEngine;
use mlua::prelude::*;

pub fn register(engine: &ScriptEngine) -> LuaResult<()> {
    let gs_arc = match engine.game_state.lock().unwrap().clone() {
        Some(gs) => gs,
        None => return Ok(()),
    };

    let lua = &engine.lua;
    let room_table = lua.create_table()?;

    let gs = gs_arc.clone();
    let mt = lua.create_table()?;
    mt.set("__index", lua.create_function(move |lua, (_t, key): (LuaTable, String)| {
        let gs = gs.read().unwrap();
        match key.as_str() {
            "title" => Ok(LuaValue::String(lua.create_string(&gs.room_name)?)),
            "description" => Ok(LuaValue::String(lua.create_string(&gs.room_description)?)),
            "exits" => {
                let t = lua.create_table()?;
                for (i, exit) in gs.room_exits.iter().enumerate() {
                    t.raw_set(i + 1, exit.as_str())?;
                }
                Ok(LuaValue::Table(t))
            }
            "id" => match gs.room_id {
                Some(id) => Ok(LuaValue::Integer(id as i64)),
                None => Ok(LuaValue::Nil),
            },
            "count" => Ok(LuaValue::Integer(gs.room_count as i64)),
            _ => Ok(LuaValue::Nil),
        }
    })?)?;
    room_table.set_metatable(Some(mt));

    lua.globals().set("Room", room_table)?;
    Ok(())
}
