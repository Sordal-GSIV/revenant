use crate::script_engine::ScriptEngine;
use mlua::prelude::*;

/// Short aliases for stat names (alias → full name).
const STAT_ALIASES: &[(&str, &str)] = &[
    ("str", "strength"),
    ("con", "constitution"),
    ("dex", "dexterity"),
    ("agi", "agility"),
    ("dis", "discipline"),
    ("aur", "aura"),
    ("log", "logic"),
    ("int", "intuition"),
    ("wis", "wisdom"),
    ("inf", "influence"),
    ("prof", "profession"),
    ("exp", "experience"),
];

const STAT_NAMES: &[&str] = &[
    "strength", "constitution", "dexterity", "agility", "discipline",
    "aura", "logic", "intuition", "wisdom", "influence",
];

fn is_stat_name(key: &str) -> bool {
    STAT_NAMES.contains(&key)
}

fn resolve_alias(key: &str) -> Option<&'static str> {
    STAT_ALIASES.iter()
        .find(|(alias, _)| *alias == key)
        .map(|(_, full)| *full)
}

pub fn register(engine: &ScriptEngine) -> LuaResult<()> {
    let lua = &engine.lua;
    let infomon = engine.infomon.clone();
    let game_state = engine.game_state.clone();

    let stats_table = lua.create_table()?;

    // __index metamethod
    let inf = infomon.clone();
    let gs = game_state.clone();
    let mt = lua.create_table()?;
    mt.set("__index", lua.create_function(move |lua, (_t, key): (LuaTable, String)| {
        let inf_guard = inf.lock().unwrap();
        let infomon = match inf_guard.as_ref() {
            Some(im) => im,
            None => return Ok(LuaValue::Nil),
        };

        let gs_opt = gs.lock().unwrap().clone();

        // Direct properties
        match key.as_str() {
            "race" => return Ok(match infomon.get("stat.race") {
                Some(v) => LuaValue::String(lua.create_string(v)?),
                None => LuaValue::Nil,
            }),
            "profession" | "prof" => return Ok(match infomon.get("stat.profession") {
                Some(v) => LuaValue::String(lua.create_string(v)?),
                None => LuaValue::Nil,
            }),
            "gender" => return Ok(match infomon.get("stat.gender") {
                Some(v) => LuaValue::String(lua.create_string(v)?),
                None => LuaValue::Nil,
            }),
            "age" => return Ok(match infomon.get("stat.age") {
                Some(v) => match v.parse::<i64>() {
                    Ok(n) => LuaValue::Integer(n),
                    Err(_) => LuaValue::Nil,
                },
                None => LuaValue::Nil,
            }),
            "level" => {
                if let Some(ref gs_arc) = gs_opt {
                    return Ok(LuaValue::Integer(gs_arc.read().unwrap().level as i64));
                }
                return Ok(LuaValue::Integer(0));
            }
            "experience" | "exp" => {
                if let Some(ref gs_arc) = gs_opt {
                    return Ok(LuaValue::Integer(gs_arc.read().unwrap().experience as i64));
                }
                return Ok(LuaValue::Integer(0));
            }
            _ => {}
        }

        let key_str = key.as_str();

        // Check if it's a full stat name → return full table
        if is_stat_name(key_str) {
            return build_full_stat_table(lua, infomon, key_str);
        }

        // Check for alias (e.g., "str" → "strength") → return {value, bonus}
        if let Some(full_name) = resolve_alias(key_str) {
            if is_stat_name(full_name) {
                return build_short_stat(lua, infomon, full_name);
            }
        }

        // Check for base_XXX or enhanced_XXX aliases
        if let Some(rest) = key_str.strip_prefix("base_") {
            let full = resolve_alias(rest).unwrap_or(rest);
            if is_stat_name(full) {
                return build_variant_stat(lua, infomon, full, "base");
            }
        }
        if let Some(rest) = key_str.strip_prefix("enhanced_") {
            let full = resolve_alias(rest).unwrap_or(rest);
            if is_stat_name(full) {
                return build_variant_stat(lua, infomon, full, "enhanced");
            }
        }

        Ok(LuaValue::Nil)
    })?)?;
    stats_table.set_metatable(Some(mt));

    lua.globals().set("Stats", stats_table)?;
    Ok(())
}

/// Build the full stat table: { value, bonus, base={value,bonus}, enhanced={value,bonus} }
fn build_full_stat_table(lua: &Lua, infomon: &crate::infomon::Infomon, name: &str) -> LuaResult<LuaValue> {
    let val = infomon.get_i64(&format!("stat.{name}"));
    let bonus = infomon.get_i64(&format!("stat.{name}_bonus"));

    let t = lua.create_table()?;
    t.set("value", val)?;
    t.set("bonus", bonus)?;

    // Base sub-table (may not exist if only "info" was used, not "info full")
    let base_val = infomon.get_i64(&format!("stat.{name}.base"));
    let base_bonus = infomon.get_i64(&format!("stat.{name}.base_bonus"));
    let base_t = lua.create_table()?;
    base_t.set("value", base_val)?;
    base_t.set("bonus", base_bonus)?;
    t.set("base", base_t)?;

    // Enhanced sub-table
    let enh_val = infomon.get_i64(&format!("stat.{name}.enhanced"));
    let enh_bonus = infomon.get_i64(&format!("stat.{name}.enhanced_bonus"));
    let enh_t = lua.create_table()?;
    enh_t.set("value", enh_val)?;
    enh_t.set("bonus", enh_bonus)?;
    t.set("enhanced", enh_t)?;

    Ok(LuaValue::Table(t))
}

/// Build short stat array: {value, bonus} (1-indexed)
fn build_short_stat(lua: &Lua, infomon: &crate::infomon::Infomon, name: &str) -> LuaResult<LuaValue> {
    let val = infomon.get_i64(&format!("stat.{name}"));
    let bonus = infomon.get_i64(&format!("stat.{name}_bonus"));
    let t = lua.create_table()?;
    t.raw_set(1, val)?;
    t.raw_set(2, bonus)?;
    Ok(LuaValue::Table(t))
}

/// Build base/enhanced variant: {value, bonus}
fn build_variant_stat(lua: &Lua, infomon: &crate::infomon::Infomon, name: &str, variant: &str) -> LuaResult<LuaValue> {
    let val = infomon.get_i64(&format!("stat.{name}.{variant}"));
    let bonus = infomon.get_i64(&format!("stat.{name}.{variant}_bonus"));
    let t = lua.create_table()?;
    t.raw_set(1, val)?;
    t.raw_set(2, bonus)?;
    Ok(LuaValue::Table(t))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_stats_lua_api() {
        let engine = ScriptEngine::new();

        // Set up Infomon with pre-populated data
        let db = crate::db::Db::open(":memory:").unwrap();
        db.set_char_data("Test", "GS3", "stat.race", "Human").unwrap();
        db.set_char_data("Test", "GS3", "stat.profession", "Wizard").unwrap();
        db.set_char_data("Test", "GS3", "stat.strength", "87").unwrap();
        db.set_char_data("Test", "GS3", "stat.strength_bonus", "12").unwrap();
        db.set_char_data("Test", "GS3", "stat.strength.enhanced", "92").unwrap();
        db.set_char_data("Test", "GS3", "stat.strength.enhanced_bonus", "16").unwrap();
        db.set_char_data("Test", "GS3", "stat.strength.base", "80").unwrap();
        db.set_char_data("Test", "GS3", "stat.strength.base_bonus", "10").unwrap();

        let infomon = crate::infomon::Infomon::new(db, "Test", "GS3");
        *engine.infomon.lock().unwrap() = Some(infomon);

        // Set up GameState
        let gs = std::sync::Arc::new(std::sync::RwLock::new(crate::game_state::GameState {
            level: 100,
            experience: 12345678,
            ..Default::default()
        }));
        engine.set_game_state(gs);

        engine.install_lua_api().unwrap();

        engine.eval_lua(r#"
            assert(Stats.race == "Human", "race: " .. tostring(Stats.race))
            assert(Stats.profession == "Wizard")
            assert(Stats.prof == "Wizard")
            assert(Stats.level == 100)

            local s = Stats.strength
            assert(s.value == 87, "str value: " .. tostring(s.value))
            assert(s.bonus == 12)
            assert(s.base.value == 80)
            assert(s.enhanced.bonus == 16)

            local short = Stats.str
            assert(short[1] == 87, "short[1]: " .. tostring(short[1]))
            assert(short[2] == 12)

            local base = Stats.base_str
            assert(base[1] == 80)
            assert(base[2] == 10)
        "#).await.unwrap();
    }
}
