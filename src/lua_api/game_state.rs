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
            // --- Already exposed (unchanged) ---
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
            "standing"   => Ok(LuaValue::Boolean(gs.standing)),
            "poisoned"   => Ok(LuaValue::Boolean(gs.poisoned)),
            "diseased"   => Ok(LuaValue::Boolean(gs.diseased)),
            "hidden"     => Ok(LuaValue::Boolean(gs.hidden)),
            "invisible"  => Ok(LuaValue::Boolean(gs.invisible)),
            "webbed"     => Ok(LuaValue::Boolean(gs.webbed)),
            "joined"     => Ok(LuaValue::Boolean(gs.joined)),
            "grouped"    => Ok(LuaValue::Boolean(gs.joined)),
            "calmed"     => Ok(LuaValue::Boolean(gs.calmed)),
            "cutthroat"  => Ok(LuaValue::Boolean(gs.cutthroat)),
            "silenced"   => Ok(LuaValue::Boolean(gs.silenced)),
            "bound"      => Ok(LuaValue::Boolean(gs.bound)),
            "room_name"     => Ok(LuaValue::String(lua.create_string(&gs.room_name)?)),
            "prompt"        => Ok(LuaValue::String(lua.create_string(&gs.prompt)?)),
            "level"         => Ok(LuaValue::Integer(gs.level as i64)),

            // --- Newly exposed ---
            "concentration"     => Ok(LuaValue::Integer(gs.concentration as i64)),
            "max_concentration" => Ok(LuaValue::Integer(gs.max_concentration as i64)),
            "room_description"  => Ok(LuaValue::String(lua.create_string(&gs.room_description)?)),
            "room_exits_string" => Ok(LuaValue::String(lua.create_string(&gs.room_exits_string)?)),
            "room_exits" => {
                let t = lua.create_table()?;
                for (i, exit) in gs.room_exits.iter().enumerate() {
                    t.raw_set(i + 1, exit.as_str())?;
                }
                Ok(LuaValue::Table(t))
            }
            "room_id" => match gs.room_id {
                Some(id) => Ok(LuaValue::Integer(id as i64)),
                None => Ok(LuaValue::Nil),
            },
            "room_count" => Ok(LuaValue::Integer(gs.room_count as i64)),
            "prepared_spell" => match &gs.prepared_spell {
                Some(s) => Ok(LuaValue::String(lua.create_string(s)?)),
                None => Ok(LuaValue::Nil),
            },
            "active_spells" => {
                let t = lua.create_table()?;
                for (i, spell) in gs.active_spells.iter().enumerate() {
                    let entry = lua.create_table()?;
                    entry.set("name", spell.name.as_str())?;
                    match spell.duration_secs {
                        Some(d) => entry.set("duration", d as i64)?,
                        None => entry.set("duration", LuaValue::Nil)?,
                    }
                    t.raw_set(i + 1, entry)?;
                }
                Ok(LuaValue::Table(t))
            }
            "stance" => match gs.stance.as_str() {
                Some(s) => Ok(LuaValue::String(lua.create_string(s)?)),
                None => Ok(LuaValue::Nil),
            },
            "stance_value" => match gs.stance.to_value() {
                Some(v) => Ok(LuaValue::Integer(v)),
                None => Ok(LuaValue::Nil),
            },
            "mind"            => Ok(LuaValue::String(lua.create_string(gs.mind.as_str())?)),
            "mind_value"      => Ok(LuaValue::Integer(gs.mind.to_value())),
            "encumbrance"       => Ok(LuaValue::String(lua.create_string(gs.encumbrance.as_str())?)),
            "encumbrance_value" => Ok(LuaValue::Integer(gs.encumbrance.to_value())),
            "server_time"     => Ok(LuaValue::Integer(gs.server_time)),
            "name"            => Ok(LuaValue::String(lua.create_string(&gs.name)?)),
            "game"            => Ok(LuaValue::String(lua.create_string(gs.game.as_str())?)),
            "experience"      => Ok(LuaValue::Integer(gs.experience as i64)),
            "right_hand_noun" => match &gs.right_hand {
                Some(s) => Ok(LuaValue::String(lua.create_string(s)?)),
                None => Ok(LuaValue::Nil),
            },
            "left_hand_noun" => match &gs.left_hand {
                Some(s) => Ok(LuaValue::String(lua.create_string(s)?)),
                None => Ok(LuaValue::Nil),
            },
            "login_time" => {
                let elapsed = gs.login_time.elapsed().as_secs_f64();
                Ok(LuaValue::Number(elapsed))
            },
            "stow_container_id" => match &gs.stow_container_id {
                Some(id) => Ok(LuaValue::String(lua.create_string(id)?)),
                None => Ok(LuaValue::Nil),
            },
            "last_pulse" => match gs.last_pulse {
                Some(t) => Ok(LuaValue::Number(t.elapsed().as_secs_f64())),
                None => Ok(LuaValue::Nil),
            },
            "wound_gsl" => Ok(LuaValue::String(lua.create_string(&gs.wound_gsl())?)),
            "scar_gsl" => Ok(LuaValue::String(lua.create_string(&gs.scar_gsl())?)),
            _ => Ok(LuaValue::Nil),
        }
    })?)?;
    gs_table.set_metatable(Some(mt));

    lua.globals().set("GameState", gs_table)?;

    // -- Wounds table --
    let gs = gs_arc.clone();
    let wounds_table = lua.create_table()?;
    let wounds_mt = lua.create_table()?;
    wounds_mt.set("__index", lua.create_function(move |_, (_t, key): (LuaTable, String)| {
        let gs = gs.read().unwrap();
        match gs.wounds.get(&key) {
            Some(v) => Ok(LuaValue::Integer(v as i64)),
            None => Ok(LuaValue::Nil),
        }
    })?)?;
    wounds_table.set_metatable(Some(wounds_mt));
    lua.globals().set("Wounds", wounds_table)?;

    // -- Scars table --
    let gs = gs_arc.clone();
    let scars_table = lua.create_table()?;
    let scars_mt = lua.create_table()?;
    scars_mt.set("__index", lua.create_function(move |_, (_t, key): (LuaTable, String)| {
        let gs = gs.read().unwrap();
        match gs.scars.get(&key) {
            Some(v) => Ok(LuaValue::Integer(v as i64)),
            None => Ok(LuaValue::Nil),
        }
    })?)?;
    scars_table.set_metatable(Some(scars_mt));
    lua.globals().set("Scars", scars_table)?;

    Ok(())
}
