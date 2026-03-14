pub mod char;
pub mod crypto;
pub mod file;
pub mod game_state;
pub mod game_obj;
pub mod hooks;
pub mod http;
pub mod json;
pub mod map;
pub mod primitives;
pub mod script;
pub mod settings;
pub mod version;

use crate::script_engine::ScriptEngine;
use anyhow::Result;

pub fn register_all(engine: &ScriptEngine) -> Result<()> {
    primitives::register(engine)?;
    game_state::register(engine)?;
    char::register(engine).map_err(|e| anyhow::anyhow!("char register: {e}"))?;
    hooks::register(engine)?;
    crypto::register(engine).map_err(|e| anyhow::anyhow!("crypto register: {e}"))?;
    file::register(engine).map_err(|e| anyhow::anyhow!("file register: {e}"))?;
    http::register(engine).map_err(|e| anyhow::anyhow!("http register: {e}"))?;
    map::register(engine).map_err(|e| anyhow::anyhow!("map register: {e}"))?;
    game_obj::register(engine).map_err(|e| anyhow::anyhow!("game_obj register: {e}"))?;
    script::register(engine)?;
    settings::register(engine)?;
    json::register(engine).map_err(|e| anyhow::anyhow!("json register: {e}"))?;
    version::register(engine).map_err(|e| anyhow::anyhow!("version register: {e}"))?;
    Ok(())
}
