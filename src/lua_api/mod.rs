pub mod primitives;
pub mod game_state;
pub mod game_obj;
pub mod hooks;
pub mod script;
pub mod map;
pub mod settings;

use anyhow::Result;

/// Register all Lua API globals on the given ScriptEngine.
/// This is a stub — individual API modules will fill this in via later tasks.
pub fn register_all(_engine: &crate::script_engine::ScriptEngine) -> Result<()> {
    Ok(())
}
