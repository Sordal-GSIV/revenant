pub mod game_state;
pub mod game_obj;
pub mod hooks;
pub mod map;
pub mod primitives;
pub mod script;
pub mod settings;

use crate::script_engine::ScriptEngine;
use anyhow::Result;

pub fn register_all(engine: &ScriptEngine) -> Result<()> {
    primitives::register(engine)?;
    if engine.game_state.lock().unwrap().is_some() {
        game_state::register(engine)?;
    }
    // hooks::register(engine)?;
    // script::register(engine)?;
    // settings::register(engine)?;
    Ok(())
}
