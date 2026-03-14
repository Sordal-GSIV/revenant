use crate::game_state::GameState;
use crate::script_engine::ScriptEngine;
use crate::spell_data::{self, SpellDef};
use mlua::prelude::*;
use std::sync::{Arc, RwLock};

/// Build a Lua table for a single spell definition + live state.
fn build_spell_table(
    lua: &Lua,
    spell: &SpellDef,
    gs: &Option<Arc<RwLock<GameState>>>,
    infomon: Option<&crate::infomon::Infomon>,
    level: u32,
) -> LuaResult<LuaTable> {
    let t = lua.create_table()?;
    t.set("num", spell.num as i64)?;
    t.set("name", spell.name.as_str())?;
    t.set("type", spell.spell_type.as_str())?;
    t.set("circle", spell.circle.as_str())?;
    t.set("circle_name", spell_data::circle_name(spell_data::spell_circle(spell.num)))?;
    t.set("availability", spell.availability.as_str())?;
    t.set("stackable", spell.stackable)?;
    t.set("persist_on_death", spell.persist_on_death)?;

    // Live: active check
    let mut is_active = false;
    let mut secs_left: f64 = 0.0;
    if let Some(ref gs_arc) = gs {
        let state = gs_arc.read().unwrap();
        if let Some(entry) = state.active_spells.iter().find(|s| s.name == spell.name) {
            is_active = true;
            if let Some(dur) = entry.duration_secs {
                let elapsed = entry.activated_at.elapsed().as_secs_f64();
                secs_left = (dur as f64 - elapsed).max(0.0);
            }
        }
    }
    t.set("active", is_active)?;
    t.set("timeleft", secs_left / 60.0)?; // minutes
    t.set("secsleft", secs_left)?;

    // Live: known check
    let mut known = false;
    if let Some(im) = infomon {
        let mut circle_ranks = std::collections::HashMap::new();
        for (k, v) in im.get_prefix("spell.") {
            let circle_name = k.strip_prefix("spell.").unwrap_or(k);
            if let Ok(n) = v.parse::<i64>() {
                circle_ranks.insert(circle_name.to_string(), n);
            }
        }
        known = spell_data::is_known(spell, &circle_ranks, level);
    }
    t.set("known", known)?;

    Ok(t)
}

pub fn register(engine: &ScriptEngine) -> LuaResult<()> {
    let lua = &engine.lua;
    let spell_list = engine.spell_list.clone();
    let game_state = engine.game_state.clone();
    let infomon = engine.infomon.clone();

    let spell_table = lua.create_table()?;

    // Spell.active() — returns array of active spell tables
    {
        let sl = spell_list.clone();
        let gs = game_state.clone();
        let inf = infomon.clone();
        spell_table.set("active", lua.create_function(move |lua, ()| {
            let gs_opt = gs.lock().unwrap().clone();
            let inf_guard = inf.lock().unwrap();
            let active_entries: Vec<(String, Option<u32>, std::time::Instant)> = match &gs_opt {
                Some(gs_arc) => {
                    let state = gs_arc.read().unwrap();
                    state.active_spells.iter()
                        .map(|s| (s.name.clone(), s.duration_secs, s.activated_at))
                        .collect()
                }
                None => Vec::new(),
            };
            let level = match &gs_opt {
                Some(gs_arc) => gs_arc.read().unwrap().level,
                None => 0,
            };

            let result = lua.create_table()?;
            let sl_guard = sl.read().unwrap();

            for (i, (name, duration, activated_at)) in active_entries.iter().enumerate() {
                let secs_left = match duration {
                    Some(dur) => (*dur as f64 - activated_at.elapsed().as_secs_f64()).max(0.0),
                    None => 0.0,
                };

                // Use build_spell_table when a matching SpellDef exists, then override live timing fields
                let t = if let Some(ref sl) = *sl_guard {
                    if let Some(spell) = sl.get_by_name(name) {
                        let t = build_spell_table(lua, spell, &gs_opt, inf_guard.as_ref(), level)?;
                        // Override with live timing values computed from activated_at
                        t.set("timeleft", secs_left / 60.0)?;
                        t.set("secsleft", secs_left)?;
                        t.set("active", true)?;
                        t
                    } else {
                        let t = lua.create_table()?;
                        t.set("name", name.as_str())?;
                        t.set("timeleft", secs_left / 60.0)?;
                        t.set("secsleft", secs_left)?;
                        t.set("active", true)?;
                        t
                    }
                } else {
                    let t = lua.create_table()?;
                    t.set("name", name.as_str())?;
                    t.set("timeleft", secs_left / 60.0)?;
                    t.set("secsleft", secs_left)?;
                    t.set("active", true)?;
                    t
                };
                result.raw_set(i + 1, t)?;
            }
            Ok(result)
        })?)?;
    }

    // Spell.active_p(num) — is spell number active?
    {
        let gs2 = game_state.clone();
        let sl2 = spell_list.clone();
        spell_table.set("active_p", lua.create_function(move |_, num: i64| {
            let gs_opt = gs2.lock().unwrap().clone();
            let sl_guard = sl2.read().unwrap();

            let spell_name = match &*sl_guard {
                Some(sl) => sl.get_by_num(num as u32).map(|s| s.name.clone()),
                None => None,
            };

            match (&gs_opt, spell_name) {
                (Some(gs_arc), Some(name)) => {
                    let state = gs_arc.read().unwrap();
                    Ok(state.active_spells.iter().any(|s| s.name == name))
                }
                _ => Ok(false),
            }
        })?)?;
    }

    // Spell.known_p(num) — is spell number known?
    {
        let inf3 = infomon.clone();
        let sl3 = spell_list.clone();
        let gs3 = game_state.clone();
        spell_table.set("known_p", lua.create_function(move |_, num: i64| {
            let sl_guard = sl3.read().unwrap();
            let gs_opt = gs3.lock().unwrap().clone();  // game_state first
            let inf_guard = inf3.lock().unwrap();       // then infomon
            let level = match &gs_opt {
                Some(gs_arc) => gs_arc.read().unwrap().level,
                None => 0,
            };
            match (&*sl_guard, inf_guard.as_ref()) {
                (Some(sl), Some(im)) => {
                    match sl.get_by_num(num as u32) {
                        Some(spell) => {
                            let mut circle_ranks = std::collections::HashMap::new();
                            for (k, v) in im.get_prefix("spell.") {
                                let cn = k.strip_prefix("spell.").unwrap_or(k);
                                if let Ok(n) = v.parse::<i64>() {
                                    circle_ranks.insert(cn.to_string(), n);
                                }
                            }
                            Ok(spell_data::is_known(spell, &circle_ranks, level))
                        }
                        None => Ok(false),
                    }
                }
                _ => Ok(false),
            }
        })?)?;
    }

    // __index metamethod: Spell[101] or Spell["Spirit Warding I"]
    {
        let sl4 = spell_list.clone();
        let gs4 = game_state.clone();
        let inf4 = infomon.clone();
        let mt = lua.create_table()?;
        mt.set("__index", lua.create_function(move |lua, (_t, key): (LuaTable, LuaValue)| {
            let sl_guard = sl4.read().unwrap();
            let sl = match &*sl_guard {
                Some(sl) => sl,
                None => return Ok(LuaValue::Nil),
            };

            let spell = match &key {
                LuaValue::Integer(n) => sl.get_by_num(*n as u32),
                LuaValue::String(s) => {
                    let name = s.to_str().map(|b| b.to_string()).unwrap_or_default();
                    sl.get_by_name(&name)
                }
                _ => None,
            };

            match spell {
                Some(spell) => {
                    let gs_opt = gs4.lock().unwrap().clone();
                    let inf_guard = inf4.lock().unwrap();
                    let level = match &gs_opt {
                        Some(gs_arc) => gs_arc.read().unwrap().level,
                        None => 0,
                    };
                    let t = build_spell_table(lua, spell, &gs_opt, inf_guard.as_ref(), level)?;
                    Ok(LuaValue::Table(t))
                }
                None => Ok(LuaValue::Nil),
            }
        })?)?;
        spell_table.set_metatable(Some(mt));
    }

    lua.globals().set("Spell", spell_table)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_spell_lookup() {
        let engine = crate::script_engine::ScriptEngine::new();

        // Set up spell list
        let xml = r#"<?xml version="1.0"?>
<effects>
  <effect num="101" name="Spirit Warding I" type="defense" circle="1"
          availability="all" stackable="false" refreshable="true"
          persist_on_death="false">
  </effect>
  <effect num="901" name="Wizard Shield" type="defense" circle="9"
          availability="self-cast" stackable="false" refreshable="true"
          persist_on_death="false">
  </effect>
</effects>"#;
        let sl = crate::spell_data::SpellList::parse(xml).unwrap();
        engine.set_spell_list(Arc::new(sl));

        // Set up Infomon with spell ranks
        let db = crate::db::Db::open(":memory:").unwrap();
        db.set_char_data("Test", "GS3", "spell.minor_spiritual", "20").unwrap();
        let infomon = crate::infomon::Infomon::new(db, "Test", "GS3");
        *engine.infomon.lock().unwrap() = Some(infomon);

        // Set up GameState
        let gs = Arc::new(std::sync::RwLock::new(crate::game_state::GameState {
            level: 100,
            ..Default::default()
        }));
        engine.set_game_state(gs);

        engine.install_lua_api().unwrap();

        engine.eval_lua(r#"
            local s = Spell[101]
            assert(s ~= nil, "Spell[101] should exist")
            assert(s.name == "Spirit Warding I", "name: " .. tostring(s.name))
            assert(s.num == 101)
            assert(s.known == true, "should be known (minor_spiritual=20, spell 101 in circle)")
            assert(s.active == false)

            local s2 = Spell["Wizard Shield"]
            assert(s2 ~= nil, "Spell by name should work")
            assert(s2.num == 901)

            assert(Spell[9999] == nil, "unknown spell should be nil")
            assert(Spell.known_p(101) == true)
            assert(Spell.active_p(101) == false)
        "#).await.unwrap();
    }
}
