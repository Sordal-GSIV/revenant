use crate::script_engine::ScriptEngine;
use mlua::prelude::*;

pub fn register(engine: &ScriptEngine) -> LuaResult<()> {
    let gs_arc = match engine.game_state.lock().unwrap().clone() {
        Some(gs) => gs,
        None => return Ok(()), // no game state attached yet
    };

    let lua = &engine.lua;
    let gs_table = lua.create_table()?;

    // roundtime() is set directly on the table
    let gs = gs_arc.clone();
    gs_table.set("roundtime", lua.create_function(move |_, ()| {
        Ok(gs.read().unwrap().roundtime())
    })?)?;

    let gs = gs_arc.clone();
    gs_table.set("cast_roundtime", lua.create_function(move |_, ()| {
        Ok(gs.read().unwrap().cast_roundtime())
    })?)?;

    // All other fields via __index metamethod
    let gs = gs_arc.clone();
    let mt = lua.create_table()?;
    mt.set("__index", lua.create_function(move |lua, (_t, key): (LuaTable, String)| {
        let gs = gs.read().unwrap();
        match key.as_str() {
            "health"        => Ok(LuaValue::Integer(gs.health as i64)),
            "max_health"    => Ok(LuaValue::Integer(gs.max_health as i64)),
            "mana"          => Ok(LuaValue::Integer(gs.mana as i64)),
            "max_mana"      => Ok(LuaValue::Integer(gs.max_mana as i64)),
            "spirit"        => Ok(LuaValue::Integer(gs.spirit as i64)),
            "max_spirit"    => Ok(LuaValue::Integer(gs.max_spirit as i64)),
            "stamina"       => Ok(LuaValue::Integer(gs.stamina as i64)),
            "max_stamina"   => Ok(LuaValue::Integer(gs.max_stamina as i64)),
            "bleeding"      => Ok(LuaValue::Boolean(gs.bleeding)),
            "stunned"       => Ok(LuaValue::Boolean(gs.stunned)),
            "dead"          => Ok(LuaValue::Boolean(gs.dead)),
            "sleeping"      => Ok(LuaValue::Boolean(gs.sleeping)),
            "prone"         => Ok(LuaValue::Boolean(gs.prone)),
            "sitting"       => Ok(LuaValue::Boolean(gs.sitting)),
            "kneeling"      => Ok(LuaValue::Boolean(gs.kneeling)),
            "room_name"     => Ok(LuaValue::String(lua.create_string(&gs.room_name)?)),
            "prompt"        => Ok(LuaValue::String(lua.create_string(&gs.prompt)?)),
            "level"         => Ok(LuaValue::Integer(gs.level as i64)),
            _               => Ok(LuaValue::Nil),
        }
    })?)?;
    gs_table.set_metatable(Some(mt));

    lua.globals().set("GameState", gs_table)?;
    Ok(())
}
