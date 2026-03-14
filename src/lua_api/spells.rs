use crate::script_engine::ScriptEngine;
use mlua::prelude::*;

/// Circle name aliases (no-underscore forms → canonical forms).
const CIRCLE_ALIASES: &[(&str, &str)] = &[
    ("minorelemental", "minor_elemental"),
    ("majorelemental", "major_elemental"),
    ("minorspiritual", "minor_spiritual"),
    ("majorspiritual", "major_spiritual"),
    ("minormental", "minor_mental"),
    ("majormental", "major_mental"),
];

pub fn register(engine: &ScriptEngine) -> LuaResult<()> {
    let lua = &engine.lua;
    let infomon = engine.infomon.clone();

    let spells_table = lua.create_table()?;

    // __index metamethod for circle rank lookups
    let inf = infomon.clone();
    let mt = lua.create_table()?;
    mt.set("__index", lua.create_function(move |_, (_t, key): (LuaTable, String)| {
        let inf_guard = inf.lock().unwrap();
        let infomon = match inf_guard.as_ref() {
            Some(im) => im,
            None => return Ok(LuaValue::Integer(0)),
        };

        let key_str = key.as_str();

        // Resolve alias
        let canonical = CIRCLE_ALIASES.iter()
            .find(|(alias, _)| *alias == key_str)
            .map(|(_, full)| *full)
            .unwrap_or(key_str);

        let db_key = format!("spell.{canonical}");
        let ranks = infomon.get_i64(&db_key);
        Ok(LuaValue::Integer(ranks))
    })?)?;
    spells_table.set_metatable(Some(mt));

    lua.globals().set("Spells", spells_table)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    #[tokio::test]
    async fn test_spells_circle_ranks() {
        let engine = crate::script_engine::ScriptEngine::new();

        let db = crate::db::Db::open(":memory:").unwrap();
        db.set_char_data("Test", "GS3", "spell.minor_elemental", "30").unwrap();
        db.set_char_data("Test", "GS3", "spell.wizard", "50").unwrap();

        let infomon = crate::infomon::Infomon::new(db, "Test", "GS3");
        *engine.infomon.lock().unwrap() = Some(infomon);

        engine.install_lua_api().unwrap();

        engine.eval_lua(r#"
            assert(Spells.minor_elemental == 30, "ranks: " .. tostring(Spells.minor_elemental))
            assert(Spells.minorelemental == 30, "alias: " .. tostring(Spells.minorelemental))
            assert(Spells.wizard == 50)
            assert(Spells.cleric == 0, "no ranks should be 0")
        "#).await.unwrap();
    }
}
