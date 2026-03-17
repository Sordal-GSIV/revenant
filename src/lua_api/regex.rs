use crate::script_engine::ScriptEngine;
use mlua::prelude::*;
use regex::Regex;
use std::sync::Arc;

/// Lua wrapper around a compiled Rust regex.
struct LuaRegex {
    inner: Arc<Regex>,
}

impl LuaRegex {
    fn new(pattern: &str) -> LuaResult<Self> {
        let re = Regex::new(pattern)
            .map_err(|e| mlua::Error::RuntimeError(format!("invalid regex: {e}")))?;
        Ok(Self { inner: Arc::new(re) })
    }
}

impl LuaUserData for LuaRegex {
    fn add_methods<M: LuaUserDataMethods<Self>>(methods: &mut M) {
        // re:test(text) → bool
        methods.add_method("test", |_, this, text: String| {
            Ok(this.inner.is_match(&text))
        });

        // re:find(text) → start, end or nil
        methods.add_method("find", |_, this, text: String| {
            match this.inner.find(&text) {
                Some(m) => Ok((Some(m.start() + 1), Some(m.end()))), // 1-indexed like Lua
                None => Ok((None, None)),
            }
        });

        // re:match(text) → matched string or nil
        methods.add_method("match", |_, this, text: String| {
            Ok(this.inner.find(&text).map(|m| m.as_str().to_string()))
        });

        // re:captures(text) → table of capture groups or nil
        methods.add_method("captures", |lua, this, text: String| {
            match this.inner.captures(&text) {
                Some(caps) => {
                    let t = lua.create_table()?;
                    // Group 0 is the full match
                    for i in 0..caps.len() {
                        if let Some(m) = caps.get(i) {
                            t.set(i, m.as_str().to_string())?;
                        }
                    }
                    // Named groups
                    for name in this.inner.capture_names().flatten() {
                        if let Some(m) = caps.name(name) {
                            t.set(name.to_string(), m.as_str().to_string())?;
                        }
                    }
                    Ok(Some(t))
                }
                None => Ok(None),
            }
        });

        // re:replace(text, replacement) → new string
        methods.add_method("replace", |_, this, (text, rep): (String, String)| {
            Ok(this.inner.replace(&text, rep.as_str()).into_owned())
        });

        // re:replace_all(text, replacement) → new string
        methods.add_method("replace_all", |_, this, (text, rep): (String, String)| {
            Ok(this.inner.replace_all(&text, rep.as_str()).into_owned())
        });

        // re:split(text) → table of substrings
        methods.add_method("split", |lua, this, text: String| {
            let parts: Vec<String> = this.inner.split(&text).map(|s| s.to_string()).collect();
            let t = lua.create_table()?;
            for (i, part) in parts.into_iter().enumerate() {
                t.set(i + 1, part)?;
            }
            Ok(t)
        });

        // re:pattern() → the original pattern string
        methods.add_method("pattern", |_, this, ()| {
            Ok(this.inner.as_str().to_string())
        });
    }
}

pub fn register(engine: &ScriptEngine) -> LuaResult<()> {
    let lua = &engine.lua;
    let re_table = lua.create_table()?;

    // Regex.new(pattern) → compiled regex object
    re_table.set("new", lua.create_function(|_, pattern: String| {
        LuaRegex::new(&pattern)
    })?)?;

    // Regex.match(pattern, text) → matched string or nil (convenience)
    re_table.set("match", lua.create_function(|_, (pattern, text): (String, String)| {
        let re = Regex::new(&pattern)
            .map_err(|e| mlua::Error::RuntimeError(format!("invalid regex: {e}")))?;
        Ok(re.find(&text).map(|m| m.as_str().to_string()))
    })?)?;

    // Regex.test(pattern, text) → bool (convenience)
    re_table.set("test", lua.create_function(|_, (pattern, text): (String, String)| {
        let re = Regex::new(&pattern)
            .map_err(|e| mlua::Error::RuntimeError(format!("invalid regex: {e}")))?;
        Ok(re.is_match(&text))
    })?)?;

    // Regex.replace(pattern, text, replacement) → new string (convenience)
    re_table.set("replace", lua.create_function(|_, (pattern, text, rep): (String, String, String)| {
        let re = Regex::new(&pattern)
            .map_err(|e| mlua::Error::RuntimeError(format!("invalid regex: {e}")))?;
        Ok(re.replace(&text, rep.as_str()).into_owned())
    })?)?;

    // Regex.replace_all(pattern, text, replacement) → new string (convenience)
    re_table.set("replace_all", lua.create_function(|_, (pattern, text, rep): (String, String, String)| {
        let re = Regex::new(&pattern)
            .map_err(|e| mlua::Error::RuntimeError(format!("invalid regex: {e}")))?;
        Ok(re.replace_all(&text, rep.as_str()).into_owned())
    })?)?;

    // Regex.split(pattern, text) → table (convenience)
    re_table.set("split", lua.create_function(|lua, (pattern, text): (String, String)| {
        let re = Regex::new(&pattern)
            .map_err(|e| mlua::Error::RuntimeError(format!("invalid regex: {e}")))?;
        let parts: Vec<String> = re.split(&text).map(|s| s.to_string()).collect();
        let t = lua.create_table()?;
        for (i, part) in parts.into_iter().enumerate() {
            t.set(i + 1, part)?;
        }
        Ok(t)
    })?)?;

    lua.globals().set("Regex", re_table)?;
    Ok(())
}
