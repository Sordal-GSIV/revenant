use mlua::prelude::*;
use std::time::Instant;
use crate::script_engine::ScriptEngine;

pub fn register(engine: &ScriptEngine) -> LuaResult<()> {
    let gs_arc = match engine.game_state.lock().unwrap().clone() {
        Some(gs) => gs,
        None => return Ok(()),
    };

    let lua = &engine.lua;
    let effects_table = lua.create_table()?;

    let dialog_names = [
        ("Buffs", "Buffs"),
        ("Debuffs", "Debuffs"),
        ("Cooldowns", "Cooldowns"),
        ("Spells", "Active Spells"),
    ];

    for (lua_name, dialog_id) in dialog_names {
        let sub = lua.create_table()?;
        let gs = gs_arc.clone();
        let did = dialog_id.to_string();

        // active(name) -> bool
        let gs2 = gs.clone();
        let did2 = did.clone();
        sub.set("active", lua.create_function(move |_, name: String| {
            let guard = gs2.read().unwrap();
            if let Some(dialog) = guard.effects.get(&did2) {
                if let Some(expiry) = dialog.get(&name) {
                    return Ok(*expiry > Instant::now());
                }
            }
            Ok(false)
        })?)?;

        // time_left(name) -> float minutes
        let gs3 = gs.clone();
        let did3 = did.clone();
        sub.set("time_left", lua.create_function(move |_, name: String| {
            let guard = gs3.read().unwrap();
            if let Some(dialog) = guard.effects.get(&did3) {
                if let Some(expiry) = dialog.get(&name) {
                    let now = Instant::now();
                    if *expiry > now {
                        let remaining = expiry.duration_since(now);
                        return Ok(remaining.as_secs_f64() / 60.0);
                    }
                }
            }
            Ok(0.0)
        })?)?;

        // to_h() -> table { name = seconds_remaining, ... }
        let gs4 = gs.clone();
        let did4 = did.clone();
        sub.set("to_h", lua.create_function(move |lua, ()| {
            let guard = gs4.read().unwrap();
            let result = lua.create_table()?;
            if let Some(dialog) = guard.effects.get(&did4) {
                let now = Instant::now();
                for (name, expiry) in dialog {
                    if *expiry > now {
                        let remaining = expiry.duration_since(now).as_secs_f64();
                        result.set(name.as_str(), remaining)?;
                    }
                }
            }
            Ok(LuaValue::Table(result))
        })?)?;

        effects_table.set(lua_name, sub)?;
    }

    lua.globals().set("Effects", effects_table)?;
    Ok(())
}
