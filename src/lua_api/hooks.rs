use crate::hook_chain::HookChain;
use crate::script_engine::ScriptEngine;
use mlua::prelude::*;
use std::sync::{Arc, Mutex};

pub fn register(engine: &ScriptEngine) -> LuaResult<()> {
    register_hook_table(engine, "DownstreamHook", engine.downstream_hooks.clone())?;
    register_hook_table(engine, "UpstreamHook", engine.upstream_hooks.clone())?;
    Ok(())
}

fn register_hook_table(
    engine: &ScriptEngine,
    global: &str,
    chain: Arc<Mutex<HookChain>>,
) -> LuaResult<()> {
    let lua = &engine.lua;
    let t = lua.create_table()?;

    let c = chain.clone();
    t.set("add", lua.create_function(move |lua, (name, func): (String, LuaFunction)| {
        let key = lua.create_registry_value(func)?;
        c.lock().unwrap().add_lua(name, key);
        Ok(())
    })?)?;

    let c = chain.clone();
    t.set("remove", lua.create_function(move |_, name: String| {
        c.lock().unwrap().remove(&name);
        Ok(())
    })?)?;

    let c = chain.clone();
    t.set("list", lua.create_function(move |lua, ()| {
        let names = c.lock().unwrap().hook_names();
        let t = lua.create_table()?;
        for (i, name) in names.iter().enumerate() {
            t.raw_set(i + 1, name.as_str())?;
        }
        Ok(t)
    })?)?;

    lua.globals().set(global, t)?;
    Ok(())
}
