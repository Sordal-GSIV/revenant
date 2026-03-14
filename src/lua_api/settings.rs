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

    // __newindex: coerce any Lua value to string via Lua's tostring()
    let (db2, c2, g2) = (db.clone(), char.clone(), game.clone());
    mt.set("__newindex", lua.create_function(move |lua, (_t, key, val): (LuaTable, String, LuaValue)| {
        let tostring: LuaFunction = lua.globals().get("tostring")?;
        let s: String = tostring.call(val)?;
        let guard = db2.lock().unwrap();
        let db = match guard.as_ref() { Some(d) => d, None => return Ok(()) };
        db.set_char_setting(&c2.lock().unwrap(), &g2.lock().unwrap(), &key, &s)
            .map_err(|e| LuaError::RuntimeError(e.to_string()))
    })?)?;

    t.set_metatable(Some(mt));
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
        let tostring: LuaFunction = lua.globals().get("tostring")?;
        let s: String = tostring.call(val)?;
        let guard = db2.lock().unwrap();
        let db = match guard.as_ref() { Some(d) => d, None => return Ok(()) };
        db.set_user_var(&g2.lock().unwrap(), &key, &s)
            .map_err(|e| LuaError::RuntimeError(e.to_string()))
    })?)?;

    t.set_metatable(Some(mt));
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
