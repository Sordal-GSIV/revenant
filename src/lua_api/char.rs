use crate::script_engine::ScriptEngine;
use mlua::prelude::*;

pub fn register(engine: &ScriptEngine) -> LuaResult<()> {
    let gs_arc = match engine.game_state.lock().unwrap().clone() {
        Some(gs) => gs,
        None => return Ok(()),
    };

    let lua = &engine.lua;
    let char_table = lua.create_table()?;

    // roundtime() and cast_roundtime() as direct functions
    let gs = gs_arc.clone();
    char_table.set("roundtime", lua.create_function(move |_, ()| {
        Ok(gs.read().unwrap().roundtime())
    })?)?;

    let gs = gs_arc.clone();
    char_table.set("cast_roundtime", lua.create_function(move |_, ()| {
        Ok(gs.read().unwrap().cast_roundtime())
    })?)?;

    // All other fields via __index
    let gs = gs_arc.clone();
    let infomon = engine.infomon.clone();
    let mt = lua.create_table()?;
    mt.set("__index", lua.create_function(move |lua, (_t, key): (LuaTable, String)| {
        let gs = gs.read().unwrap();
        let inf_guard = infomon.lock().unwrap();
        match key.as_str() {
            "name"       => Ok(LuaValue::String(lua.create_string(&gs.name)?)),
            "health"     => Ok(LuaValue::Integer(gs.health as i64)),
            "max_health" => Ok(LuaValue::Integer(gs.max_health as i64)),
            "percent_health" => Ok(LuaValue::Integer(
                if gs.max_health > 0 { (gs.health as i64) * 100 / (gs.max_health as i64) } else { 0 }
            )),
            "mana"       => Ok(LuaValue::Integer(gs.mana as i64)),
            "max_mana"   => Ok(LuaValue::Integer(gs.max_mana as i64)),
            "percent_mana" => Ok(LuaValue::Integer(
                if gs.max_mana > 0 { (gs.mana as i64) * 100 / (gs.max_mana as i64) } else { 0 }
            )),
            "spirit"     => Ok(LuaValue::Integer(gs.spirit as i64)),
            "max_spirit" => Ok(LuaValue::Integer(gs.max_spirit as i64)),
            "percent_spirit" => Ok(LuaValue::Integer(
                if gs.max_spirit > 0 { (gs.spirit as i64) * 100 / (gs.max_spirit as i64) } else { 0 }
            )),
            "stamina"    => Ok(LuaValue::Integer(gs.stamina as i64)),
            "max_stamina" => Ok(LuaValue::Integer(gs.max_stamina as i64)),
            "percent_stamina" => Ok(LuaValue::Integer(
                if gs.max_stamina > 0 { (gs.stamina as i64) * 100 / (gs.max_stamina as i64) } else { 0 }
            )),
            "stance" => match gs.stance.as_str() {
                Some(s) => Ok(LuaValue::String(lua.create_string(s)?)),
                None => Ok(LuaValue::Nil),
            },
            "stance_value" => match gs.stance.to_value() {
                Some(v) => Ok(LuaValue::Integer(v)),
                None => Ok(LuaValue::Nil),
            },
            "encumbrance"       => Ok(LuaValue::String(lua.create_string(gs.encumbrance.as_str())?)),
            "encumbrance_value" => Ok(LuaValue::Integer(gs.encumbrance.to_value())),
            "level"      => Ok(LuaValue::Integer(gs.level as i64)),
            "experience" => Ok(LuaValue::Integer(gs.experience as i64)),
            // Status booleans
            "dead"       => Ok(LuaValue::Boolean(gs.dead)),
            "stunned"    => Ok(LuaValue::Boolean(gs.stunned)),
            "bleeding"   => Ok(LuaValue::Boolean(gs.bleeding)),
            "sleeping"   => Ok(LuaValue::Boolean(gs.sleeping)),
            "prone"      => Ok(LuaValue::Boolean(gs.prone)),
            "sitting"    => Ok(LuaValue::Boolean(gs.sitting)),
            "kneeling"   => Ok(LuaValue::Boolean(gs.kneeling)),
            "citizenship" => {
                match inf_guard.as_ref().and_then(|im| im.get("citizenship")) {
                    Some(v) => Ok(LuaValue::String(lua.create_string(v)?)),
                    None => Ok(LuaValue::Nil),
                }
            }
            "che" => {
                match inf_guard.as_ref().and_then(|im| im.get("che")) {
                    Some(v) => Ok(LuaValue::String(lua.create_string(v)?)),
                    None => Ok(LuaValue::Nil),
                }
            }
            _ => Ok(LuaValue::Nil),
        }
    })?)?;
    char_table.set_metatable(Some(mt));

    lua.globals().set("Char", char_table)?;
    Ok(())
}
