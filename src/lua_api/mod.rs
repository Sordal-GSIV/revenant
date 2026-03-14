pub mod bounty;
pub mod char;
pub mod crypto;
pub mod familiar;
pub mod room;
pub mod file;
pub mod game_state;
pub mod game_obj;
pub mod group;
pub mod society;
pub mod hooks;
pub mod http;
pub mod json;
pub mod map;
pub mod primitives;
pub mod script;
pub mod settings;
pub mod skills;
pub mod spell;
pub mod spells;
pub mod stats;
pub mod version;

use crate::script_engine::ScriptEngine;
use anyhow::Result;

pub fn register_all(engine: &ScriptEngine) -> Result<()> {
    primitives::register(engine)?;
    game_state::register(engine)?;
    char::register(engine).map_err(|e| anyhow::anyhow!("char register: {e}"))?;
    room::register(engine).map_err(|e| anyhow::anyhow!("room register: {e}"))?;
    hooks::register(engine)?;
    crypto::register(engine).map_err(|e| anyhow::anyhow!("crypto register: {e}"))?;
    file::register(engine).map_err(|e| anyhow::anyhow!("file register: {e}"))?;
    http::register(engine).map_err(|e| anyhow::anyhow!("http register: {e}"))?;
    map::register(engine).map_err(|e| anyhow::anyhow!("map register: {e}"))?;
    game_obj::register(engine).map_err(|e| anyhow::anyhow!("game_obj register: {e}"))?;
    bounty::register(engine).map_err(|e| anyhow::anyhow!("bounty register: {e}"))?;
    society::register(engine).map_err(|e| anyhow::anyhow!("society register: {e}"))?;
    group::register(engine).map_err(|e| anyhow::anyhow!("group register: {e}"))?;
    familiar::register(engine).map_err(|e| anyhow::anyhow!("familiar register: {e}"))?;
    script::register(engine)?;
    settings::register(engine)?;
    stats::register(engine).map_err(|e| anyhow::anyhow!("stats register: {e}"))?;
    skills::register(engine).map_err(|e| anyhow::anyhow!("skills register: {e}"))?;
    spell::register(engine).map_err(|e| anyhow::anyhow!("spell register: {e}"))?;
    spells::register(engine).map_err(|e| anyhow::anyhow!("spells register: {e}"))?;
    json::register(engine).map_err(|e| anyhow::anyhow!("json register: {e}"))?;
    version::register(engine).map_err(|e| anyhow::anyhow!("version register: {e}"))?;
    register_lua_builtins(engine)?;
    Ok(())
}

fn register_lua_builtins(engine: &ScriptEngine) -> Result<()> {
    let builtins_src = include_str!("builtins.lua");
    engine.lua.load(builtins_src).set_name("builtins").exec()
        .map_err(|e| anyhow::anyhow!("builtins.lua: {e}"))?;
    let aliases_src = include_str!("aliases.lua");
    engine.lua.load(aliases_src).set_name("aliases").exec()
        .map_err(|e| anyhow::anyhow!("aliases.lua: {e}"))?;
    Ok(())
}
