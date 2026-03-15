use crate::script_engine::ScriptEngine;
use mlua::prelude::*;

pub fn register(engine: &ScriptEngine) -> LuaResult<()> {
    register_char_settings(engine)?;
    register_user_vars(engine)?;
    register_global_settings(engine)?;
    Ok(())
}

fn register_char_settings(engine: &ScriptEngine) -> LuaResult<()> {
    let lua = &engine.lua;
    let db = engine.db.clone();
    let char = engine.character.clone();
    let game = engine.game.clone();

    let t = lua.create_table()?;
    let mt = lua.create_table()?;

    // __index: return nil for missing keys (NOT empty string)
    let (db2, c2, g2) = (db.clone(), char.clone(), game.clone());
    mt.set("__index", lua.create_function(move |lua, (_t, key): (LuaTable, String)| {
        let guard = db2.lock().unwrap();
        let db = match guard.as_ref() { Some(d) => d, None => return Ok(LuaValue::Nil) };
        match db.get_char_setting(&c2.lock().unwrap(), &g2.lock().unwrap(), &key) {
            Ok(Some(v)) => Ok(LuaValue::String(lua.create_string(&v)?)),
            Ok(None) => Ok(LuaValue::Nil),
            Err(e) => Err(LuaError::RuntimeError(e.to_string())),
        }
    })?)?;

    // __newindex: nil deletes the key; any other value is coerced to string via tostring()
    let (db2, c2, g2) = (db.clone(), char.clone(), game.clone());
    mt.set("__newindex", lua.create_function(move |lua, (_t, key, val): (LuaTable, String, LuaValue)| {
        let guard = db2.lock().unwrap();
        let db = match guard.as_ref() { Some(d) => d, None => return Ok(()) };
        if matches!(val, LuaValue::Nil) {
            db.delete_char_setting(&c2.lock().unwrap(), &g2.lock().unwrap(), &key)
                .map_err(|e| LuaError::RuntimeError(e.to_string()))
        } else {
            let tostring: LuaFunction = lua.globals().get("tostring")?;
            let s: String = tostring.call(val)?;
            db.set_char_setting(&c2.lock().unwrap(), &g2.lock().unwrap(), &key, &s)
                .map_err(|e| LuaError::RuntimeError(e.to_string()))
        }
    })?)?;

    t.set_metatable(Some(mt));

    // list(prefix?) — returns array of {key, value} pairs with prefix stripped
    let (db2, c2, g2) = (db.clone(), char.clone(), game.clone());
    t.set("list", lua.create_function(move |lua, prefix: Option<String>| {
        let prefix = prefix.unwrap_or_default();
        let guard = db2.lock().unwrap();
        let db = match guard.as_ref() { Some(d) => d, None => return Ok(LuaValue::Table(lua.create_table()?)) };
        match db.list_char_settings(&c2.lock().unwrap(), &g2.lock().unwrap(), &prefix) {
            Ok(entries) => {
                let result = lua.create_table()?;
                for (i, (key, value)) in entries.into_iter().enumerate() {
                    let pair = lua.create_table()?;
                    let display_key = if !prefix.is_empty() && key.starts_with(&prefix) {
                        key[prefix.len()..].to_string()
                    } else {
                        key
                    };
                    pair.set(1, display_key)?;
                    pair.set(2, value)?;
                    result.set(i + 1, pair)?;
                }
                Ok(LuaValue::Table(result))
            }
            Err(e) => {
                tracing::warn!("CharSettings.list error: {e}");
                Ok(LuaValue::Table(lua.create_table()?))
            }
        }
    })?)?;

    lua.globals().set("CharSettings", t)?;
    Ok(())
}

fn register_user_vars(engine: &ScriptEngine) -> LuaResult<()> {
    let lua = &engine.lua;
    let db = engine.db.clone();
    let game = engine.game.clone();

    let t = lua.create_table()?;
    let mt = lua.create_table()?;

    let (db2, g2) = (db.clone(), game.clone());
    mt.set("__index", lua.create_function(move |lua, (_t, key): (LuaTable, String)| {
        let guard = db2.lock().unwrap();
        let db = match guard.as_ref() { Some(d) => d, None => return Ok(LuaValue::Nil) };
        match db.get_user_var(&g2.lock().unwrap(), &key) {
            Ok(Some(v)) => Ok(LuaValue::String(lua.create_string(&v)?)),
            Ok(None) => Ok(LuaValue::Nil),
            Err(e) => Err(LuaError::RuntimeError(e.to_string())),
        }
    })?)?;

    let (db2, g2) = (db.clone(), game.clone());
    mt.set("__newindex", lua.create_function(move |lua, (_t, key, val): (LuaTable, String, LuaValue)| {
        let guard = db2.lock().unwrap();
        let db = match guard.as_ref() { Some(d) => d, None => return Ok(()) };
        if matches!(val, LuaValue::Nil) {
            db.delete_user_var(&g2.lock().unwrap(), &key)
                .map_err(|e| LuaError::RuntimeError(e.to_string()))
        } else {
            let tostring: LuaFunction = lua.globals().get("tostring")?;
            let s: String = tostring.call(val)?;
            db.set_user_var(&g2.lock().unwrap(), &key, &s)
                .map_err(|e| LuaError::RuntimeError(e.to_string()))
        }
    })?)?;

    t.set_metatable(Some(mt));

    // list() — returns array of {key, value} pairs
    let (db2, g2) = (db.clone(), game.clone());
    t.set("list", lua.create_function(move |lua, ()| {
        let guard = db2.lock().unwrap();
        let db = match guard.as_ref() { Some(d) => d, None => return Ok(LuaValue::Table(lua.create_table()?)) };
        match db.list_user_vars(&g2.lock().unwrap()) {
            Ok(entries) => {
                let result = lua.create_table()?;
                for (i, (key, value)) in entries.into_iter().enumerate() {
                    let pair = lua.create_table()?;
                    pair.set(1, key)?;
                    pair.set(2, value)?;
                    result.set(i + 1, pair)?;
                }
                Ok(LuaValue::Table(result))
            }
            Err(e) => {
                tracing::warn!("UserVars.list error: {e}");
                Ok(LuaValue::Table(lua.create_table()?))
            }
        }
    })?)?;

    lua.globals().set("UserVars", t)?;
    Ok(())
}

fn register_global_settings(engine: &ScriptEngine) -> LuaResult<()> {
    let lua = &engine.lua;
    let db = engine.db.clone();
    let game = engine.game.clone();

    let t = lua.create_table()?;
    let mt = lua.create_table()?;

    // __index: look up key under the "_global_" character sentinel
    let (db2, g2) = (db.clone(), game.clone());
    mt.set("__index", lua.create_function(move |lua, (_t, key): (LuaTable, String)| {
        let guard = db2.lock().unwrap();
        let db = match guard.as_ref() { Some(d) => d, None => return Ok(LuaValue::Nil) };
        match db.get_char_setting("_global_", &g2.lock().unwrap(), &key) {
            Ok(Some(v)) => Ok(LuaValue::String(lua.create_string(&v)?)),
            Ok(None) => Ok(LuaValue::Nil),
            Err(e) => Err(LuaError::RuntimeError(e.to_string())),
        }
    })?)?;

    // __newindex: coerce any Lua value to string and store under "_global_", or delete if nil
    let (db2, g2) = (db.clone(), game.clone());
    mt.set("__newindex", lua.create_function(move |lua, (_t, key, val): (LuaTable, String, LuaValue)| {
        let guard = db2.lock().unwrap();
        let db = match guard.as_ref() { Some(d) => d, None => return Ok(()) };
        if matches!(val, LuaValue::Nil) {
            db.delete_char_setting("_global_", &g2.lock().unwrap(), &key)
                .map_err(|e| LuaError::RuntimeError(e.to_string()))
        } else {
            let tostring: LuaFunction = lua.globals().get("tostring")?;
            let s: String = tostring.call(val)?;
            db.set_char_setting("_global_", &g2.lock().unwrap(), &key, &s)
                .map_err(|e| LuaError::RuntimeError(e.to_string()))
        }
    })?)?;

    t.set_metatable(Some(mt));
    lua.globals().set("Settings", t)?;
    Ok(())
}
