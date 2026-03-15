use mlua::prelude::*;
use crate::script_engine::ScriptEngine;

pub fn register(engine: &ScriptEngine) -> LuaResult<()> {
    let lua = &engine.lua;
    let infomon = engine.infomon.clone();

    let table = lua.create_table()?;

    // Infomon.get(key) → string or nil
    let im_ref = infomon.clone();
    table.set("get", lua.create_function(move |_, key: String| {
        let guard = im_ref.lock().unwrap();
        Ok(guard.as_ref().and_then(|im| im.get(&key).map(|s| s.to_string())))
    })?)?;

    // Infomon.get_i(key) → integer (0 if missing)
    let im_ref = infomon.clone();
    table.set("get_i", lua.create_function(move |_, key: String| {
        let guard = im_ref.lock().unwrap();
        Ok(guard.as_ref().map(|im| im.get_i64(&key)).unwrap_or(0))
    })?)?;

    // Infomon.keys() → table of all cached keys
    let im_ref = infomon.clone();
    table.set("keys", lua.create_function(move |lua, ()| {
        let guard = im_ref.lock().unwrap();
        let t = lua.create_table()?;
        if let Some(im) = guard.as_ref() {
            let mut keys = im.cached_keys();
            keys.sort();
            for (i, k) in keys.iter().enumerate() {
                t.set(i + 1, k.as_str())?;
            }
        }
        Ok(t)
    })?)?;

    // Infomon.show(full) → outputs to respond sink
    let im_ref = infomon.clone();
    let respond_sink = engine.respond_sink.clone();
    table.set("show", lua.create_function(move |_, full: Option<bool>| {
        let guard = im_ref.lock().unwrap();
        if let Some(im) = guard.as_ref() {
            let lines = im.show(full.unwrap_or(false));
            let sink = respond_sink.lock().unwrap();
            if let Some(ref send_fn) = *sink {
                for line in lines {
                    send_fn(line + "\n");
                }
            }
        }
        Ok(())
    })?)?;

    // Infomon.effects() → boolean
    let im_ref = infomon.clone();
    table.set("effects", lua.create_function(move |_, ()| {
        let guard = im_ref.lock().unwrap();
        Ok(guard.as_ref()
            .and_then(|im| im.get("infomon.show_durations"))
            .map(|v| v == "true")
            .unwrap_or(false))
    })?)?;

    // Infomon.set_effects(bool)
    let im_ref = infomon.clone();
    table.set("set_effects", lua.create_function(move |_, val: bool| {
        let mut guard = im_ref.lock().unwrap();
        if let Some(ref mut im) = *guard {
            let v = if val { "true" } else { "false" };
            im.set_direct("infomon.show_durations", v);
        }
        Ok(())
    })?)?;

    // Infomon.sync() — sends all 15 game commands via upstream_sink
    let im_ref = infomon.clone();
    let upstream_sink = engine.upstream_sink.clone();
    table.set("sync", lua.create_function(move |_, ()| {
        let commands = [
            "info full", "skill", "spell", "experience", "society",
            "citizenship", "armor list all", "cman list all", "feat list all",
            "shield list all", "weapon list all", "ascension list all",
            "resource", "warcry", "profile full",
        ];
        let sink = upstream_sink.lock().unwrap();
        if let Some(ref send_fn) = *sink {
            for cmd in &commands {
                send_fn(format!("{cmd}\n"));
            }
        }
        drop(sink);
        if let Some(ref mut im) = *im_ref.lock().unwrap() {
            im.set_synced(true);
        }
        Ok(())
    })?)?;

    // Infomon.reset() — wipe DB + cache, then re-sync
    let im_ref = infomon.clone();
    let upstream_sink = engine.upstream_sink.clone();
    table.set("reset", lua.create_function(move |_, ()| {
        {
            let mut guard = im_ref.lock().unwrap();
            if let Some(ref mut im) = *guard {
                im.reset();
            }
        }
        let commands = [
            "info full", "skill", "spell", "experience", "society",
            "citizenship", "armor list all", "cman list all", "feat list all",
            "shield list all", "weapon list all", "ascension list all",
            "resource", "warcry", "profile full",
        ];
        let sink = upstream_sink.lock().unwrap();
        if let Some(ref send_fn) = *sink {
            for cmd in &commands {
                send_fn(format!("{cmd}\n"));
            }
        }
        drop(sink);
        if let Some(ref mut im) = *im_ref.lock().unwrap() {
            im.set_synced(true);
        }
        Ok(())
    })?)?;

    // Metatable for dynamic field access (synced)
    let mt = lua.create_table()?;
    let im_ref = infomon.clone();
    mt.set("__index", lua.create_function(move |_, (_t, key): (LuaTable, String)| {
        if key == "synced" {
            let guard = im_ref.lock().unwrap();
            Ok(LuaValue::Boolean(guard.as_ref().map(|im| im.is_synced()).unwrap_or(false)))
        } else {
            Ok(LuaValue::Nil)
        }
    })?)?;
    table.set_metatable(Some(mt));

    lua.globals().set("Infomon", table)?;
    Ok(())
}
