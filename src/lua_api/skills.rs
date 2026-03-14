use crate::script_engine::ScriptEngine;
use mlua::prelude::*;
use std::collections::HashMap;

/// Lich5-compatible skill bonus formula.
/// 1-10 ranks: 5 per rank. 11-20: 4 per rank. 21-30: 3 per rank.
/// 31-40: 2 per rank. 41+: 1 per rank.
pub fn skill_bonus(mut ranks: i64) -> i64 {
    let mut bonus = 0i64;
    while ranks > 0 {
        if ranks > 40 {
            bonus += ranks - 40;
            ranks = 40;
        } else if ranks > 30 {
            bonus += (ranks - 30) * 2;
            ranks = 30;
        } else if ranks > 20 {
            bonus += (ranks - 20) * 3;
            ranks = 20;
        } else if ranks > 10 {
            bonus += (ranks - 10) * 4;
            ranks = 10;
        } else {
            bonus += ranks * 5;
            ranks = 0;
        }
    }
    bonus
}

/// Build the legacy alias map (no-underscore form → canonical form).
fn build_alias_map() -> HashMap<&'static str, &'static str> {
    let mut m = HashMap::new();
    m.insert("twoweaponcombat", "two_weapon_combat");
    m.insert("armoruse", "armor_use");
    m.insert("shielduse", "shield_use");
    m.insert("combatmaneuvers", "combat_maneuvers");
    m.insert("edgedweapons", "edged_weapons");
    m.insert("bluntweapons", "blunt_weapons");
    m.insert("twohandedweapons", "two_handed_weapons");
    m.insert("rangedweapons", "ranged_weapons");
    m.insert("thrownweapons", "thrown_weapons");
    m.insert("polearmweapons", "polearm_weapons");
    m.insert("multiopponentcombat", "multi_opponent_combat");
    m.insert("physicalfitness", "physical_fitness");
    m.insert("arcanesymbols", "arcane_symbols");
    m.insert("magicitemuse", "magic_item_use");
    m.insert("spellaiming", "spell_aiming");
    m.insert("harnesspower", "harness_power");
    m.insert("disarmingtraps", "disarming_traps");
    m.insert("pickinglocks", "picking_locks");
    m.insert("stalkingandhiding", "stalking_and_hiding");
    m.insert("firstaid", "first_aid");
    m.insert("emc", "elemental_mana_control");
    m.insert("mmc", "mental_mana_control");
    m.insert("smc", "spirit_mana_control");
    m.insert("elair", "elemental_lore_air");
    m.insert("elearth", "elemental_lore_earth");
    m.insert("elfire", "elemental_lore_fire");
    m.insert("elwater", "elemental_lore_water");
    m.insert("slblessings", "spiritual_lore_blessings");
    m.insert("slreligion", "spiritual_lore_religion");
    m.insert("slsummoning", "spiritual_lore_summoning");
    m.insert("sldemonology", "sorcerous_lore_demonology");
    m.insert("slnecromancy", "sorcerous_lore_necromancy");
    m.insert("mldivination", "mental_lore_divination");
    m.insert("mlmanipulation", "mental_lore_manipulation");
    m.insert("mltelepathy", "mental_lore_telepathy");
    m.insert("mltransference", "mental_lore_transference");
    m.insert("mltransformation", "mental_lore_transformation");
    m
}

pub fn register(engine: &ScriptEngine) -> LuaResult<()> {
    let lua = &engine.lua;
    let infomon = engine.infomon.clone();
    let aliases = build_alias_map();

    let skills_table = lua.create_table()?;

    // Skills.to_bonus(ranks_or_skill_name)
    let inf = infomon.clone();
    skills_table.set("to_bonus", lua.create_function(move |_, val: LuaValue| {
        match val {
            LuaValue::Integer(ranks) => Ok(skill_bonus(ranks)),
            LuaValue::String(s) => {
                let name = s.to_str().map(|b| b.to_string()).unwrap_or_default();
                let inf_guard = inf.lock().unwrap();
                match inf_guard.as_ref() {
                    Some(im) => {
                        let bonus = im.get_i64(&format!("skill.{name}_bonus"));
                        Ok(bonus)
                    }
                    None => Ok(0),
                }
            }
            _ => Ok(0),
        }
    })?)?;

    // __index metamethod for skill lookups
    let inf = infomon.clone();
    let mt = lua.create_table()?;
    mt.set("__index", lua.create_function(move |_, (_t, key): (LuaTable, String)| {
        let inf_guard = inf.lock().unwrap();
        let infomon = match inf_guard.as_ref() {
            Some(im) => im,
            None => return Ok(LuaValue::Integer(0)),
        };

        // First try the key directly
        let db_key = format!("skill.{key}");
        if let Some(v) = infomon.get(&db_key) {
            if let Ok(n) = v.parse::<i64>() {
                return Ok(LuaValue::Integer(n));
            }
        }

        // Try legacy alias
        if let Some(canonical) = aliases.get(key.as_str()) {
            let db_key = format!("skill.{canonical}");
            if let Some(v) = infomon.get(&db_key) {
                if let Ok(n) = v.parse::<i64>() {
                    return Ok(LuaValue::Integer(n));
                }
            }
        }

        Ok(LuaValue::Integer(0))
    })?)?;
    skills_table.set_metatable(Some(mt));

    lua.globals().set("Skills", skills_table)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_skill_bonus_formula() {
        assert_eq!(skill_bonus(0), 0);
        assert_eq!(skill_bonus(1), 5);
        assert_eq!(skill_bonus(10), 50);
        assert_eq!(skill_bonus(20), 90);
        assert_eq!(skill_bonus(30), 120);
        assert_eq!(skill_bonus(40), 140);
        assert_eq!(skill_bonus(50), 150);
        assert_eq!(skill_bonus(100), 200);
    }

    #[tokio::test]
    async fn test_skills_lua_api() {
        let engine = crate::script_engine::ScriptEngine::new();

        let db = crate::db::Db::open(":memory:").unwrap();
        db.set_char_data("Test", "GS3", "skill.edged_weapons", "30").unwrap();
        db.set_char_data("Test", "GS3", "skill.edged_weapons_bonus", "140").unwrap();
        db.set_char_data("Test", "GS3", "skill.two_weapon_combat", "62").unwrap();
        db.set_char_data("Test", "GS3", "skill.two_weapon_combat_bonus", "162").unwrap();

        let infomon = crate::infomon::Infomon::new(db, "Test", "GS3");
        *engine.infomon.lock().unwrap() = Some(infomon);

        engine.install_lua_api().unwrap();

        engine.eval_lua(r#"
            assert(Skills.edged_weapons == 30, "ranks: " .. tostring(Skills.edged_weapons))
            assert(Skills.edgedweapons == 30, "alias: " .. tostring(Skills.edgedweapons))
            assert(Skills.two_weapon_combat == 62)
            assert(Skills.twoweaponcombat == 62)
            assert(Skills.to_bonus(30) == 120)
            assert(Skills.to_bonus(10) == 50)
            assert(Skills.to_bonus("edged_weapons") == 140)
        "#).await.unwrap();
    }
}
