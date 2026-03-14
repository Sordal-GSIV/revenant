use mlua::prelude::*;

use crate::script_engine::ScriptEngine;

#[allow(clippy::only_used_in_recursion)]
fn lua_value_to_json(lua: &Lua, val: LuaValue) -> LuaResult<serde_json::Value> {
    match val {
        LuaValue::Nil => Ok(serde_json::Value::Null),
        LuaValue::Boolean(b) => Ok(serde_json::Value::Bool(b)),
        LuaValue::Integer(i) => Ok(serde_json::Value::Number(
            serde_json::Number::from(i),
        )),
        LuaValue::Number(n) => Ok(serde_json::Value::Number(
            serde_json::Number::from_f64(n)
                .unwrap_or(serde_json::Number::from(0)),
        )),
        LuaValue::String(s) => Ok(serde_json::Value::String(s.to_str()?.to_string())),
        LuaValue::Table(t) => {
            // Check if it's an array (sequential integer keys starting at 1)
            let len = t.raw_len();
            if len > 0 {
                let mut arr = Vec::new();
                let mut is_array = true;
                for i in 1..=len {
                    match t.raw_get::<LuaValue>(i) {
                        Ok(v) if v != LuaValue::Nil => arr.push(lua_value_to_json(lua, v)?),
                        _ => {
                            is_array = false;
                            break;
                        }
                    }
                }
                if is_array {
                    // Check there are no other keys beyond the sequential ones
                    let mut extra_keys = false;
                    for pair in t.pairs::<LuaValue, LuaValue>() {
                        let (k, _) = pair?;
                        if let LuaValue::Integer(i) = k {
                            if i >= 1 && i <= len as i64 {
                                continue;
                            }
                        }
                        extra_keys = true;
                        break;
                    }
                    if !extra_keys {
                        return Ok(serde_json::Value::Array(arr));
                    }
                }
            }
            // Treat as object
            let mut map = serde_json::Map::new();
            for pair in t.pairs::<LuaValue, LuaValue>() {
                let (k, v) = pair?;
                let key = match k {
                    LuaValue::String(s) => s.to_str()?.to_string(),
                    LuaValue::Integer(i) => i.to_string(),
                    LuaValue::Number(n) => n.to_string(),
                    _ => continue,
                };
                map.insert(key, lua_value_to_json(lua, v)?);
            }
            Ok(serde_json::Value::Object(map))
        }
        _ => Ok(serde_json::Value::Null),
    }
}

pub fn json_value_to_lua(lua: &Lua, val: &serde_json::Value) -> LuaResult<LuaValue> {
    match val {
        serde_json::Value::Null => Ok(LuaValue::Nil),
        serde_json::Value::Bool(b) => Ok(LuaValue::Boolean(*b)),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Ok(LuaValue::Integer(i))
            } else if let Some(f) = n.as_f64() {
                Ok(LuaValue::Number(f))
            } else {
                Ok(LuaValue::Nil)
            }
        }
        serde_json::Value::String(s) => Ok(LuaValue::String(lua.create_string(s)?)),
        serde_json::Value::Array(arr) => {
            let t = lua.create_table()?;
            for (i, v) in arr.iter().enumerate() {
                t.raw_set(i + 1, json_value_to_lua(lua, v)?)?;
            }
            Ok(LuaValue::Table(t))
        }
        serde_json::Value::Object(map) => {
            let t = lua.create_table()?;
            for (k, v) in map {
                t.raw_set(k.as_str(), json_value_to_lua(lua, v)?)?;
            }
            Ok(LuaValue::Table(t))
        }
    }
}

pub fn register(engine: &ScriptEngine) -> LuaResult<()> {
    let lua = &engine.lua;
    let globals = lua.globals();

    let json_table = lua.create_table()?;

    json_table.set(
        "encode",
        lua.create_function(|lua, val: LuaValue| {
            let json_val = lua_value_to_json(lua, val)?;
            serde_json::to_string(&json_val)
                .map_err(|e| LuaError::runtime(format!("json encode error: {e}")))
        })?,
    )?;

    json_table.set(
        "decode",
        lua.create_function(|lua, s: String| {
            match serde_json::from_str::<serde_json::Value>(&s) {
                Ok(val) => Ok((Some(json_value_to_lua(lua, &val)?), None::<String>)),
                Err(e) => Ok((None, Some(format!("json decode error: {e}")))),
            }
        })?,
    )?;

    globals.set("Json", json_table)?;
    Ok(())
}
