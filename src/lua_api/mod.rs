pub mod bounty;
pub mod char;
pub mod crypto;
pub mod effects;
pub mod familiar;
pub mod room;
pub mod file;
pub mod game_state;
pub mod game_obj;
pub mod group;
pub mod society;
pub mod hooks;
pub mod http;
pub mod infomon;
pub mod json;
pub mod map;
pub mod primitives;
pub mod script;
pub mod settings;
pub mod skills;
pub mod spell;
pub mod spells;
pub mod stats;
pub mod frontend;
pub mod regex;
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
    effects::register(engine).map_err(|e| anyhow::anyhow!("effects register: {e}"))?;
    familiar::register(engine).map_err(|e| anyhow::anyhow!("familiar register: {e}"))?;
    script::register(engine)?;
    settings::register(engine)?;
    stats::register(engine).map_err(|e| anyhow::anyhow!("stats register: {e}"))?;
    skills::register(engine).map_err(|e| anyhow::anyhow!("skills register: {e}"))?;
    spell::register(engine).map_err(|e| anyhow::anyhow!("spell register: {e}"))?;
    spells::register(engine).map_err(|e| anyhow::anyhow!("spells register: {e}"))?;
    infomon::register(engine).map_err(|e| anyhow::anyhow!("infomon register: {e}"))?;
    json::register(engine).map_err(|e| anyhow::anyhow!("json register: {e}"))?;
    version::register(engine).map_err(|e| anyhow::anyhow!("version register: {e}"))?;
    frontend::register(engine).map_err(|e| anyhow::anyhow!("frontend register: {e}"))?;
    regex::register(engine).map_err(|e| anyhow::anyhow!("regex register: {e}"))?;
    crate::gui::lua_api::register(engine).map_err(|e| anyhow::anyhow!("gui register: {e}"))?;
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
    register_game_searcher(engine)?;
    Ok(())
}

/// Install a custom Lua package searcher that resolves game-specific modules.
///
/// When a script does `require("lib/bounty")`, Lua's normal searcher looks for
/// `{scripts_dir}/lib/bounty.lua`. This custom searcher runs FIRST and checks
/// `{scripts_dir}/lib/{game}/bounty.lua` (where {game} is "gs" or "dr").
///
/// If the game-specific file exists, it's loaded. Otherwise the normal searcher
/// chain continues and finds the agnostic file. This lets scripts use
/// `require("lib/bounty")` without knowing whether it's game-specific.
///
/// Also works for `data/` paths: `require("data/creatures")` resolves to
/// `data/{game}/creatures.lua` if it exists.
fn register_game_searcher(engine: &ScriptEngine) -> Result<()> {
    let scripts_dir = engine.scripts_dir.clone();
    let game_state = engine.game_state.clone();

    let searcher = engine.lua.create_function(move |lua, module_name: String| {
        // Only intercept lib/ and data/ paths
        let prefix = if module_name.starts_with("lib/") {
            "lib/"
        } else if module_name.starts_with("data/") {
            "data/"
        } else {
            return Ok(mlua::Value::String(lua.create_string(
                format!("\n\tgame-searcher: not a lib/ or data/ path: '{module_name}'")
            )?));
        };

        // Determine game subdirectory
        let game_sub = {
            let guard = game_state.lock().unwrap();
            match guard.as_ref() {
                Some(gs_arc) => {
                    let gs = gs_arc.read().unwrap_or_else(|e| e.into_inner());
                    match gs.game {
                        crate::game_state::Game::DragonRealms => "dr",
                        crate::game_state::Game::GemStone => "gs",
                    }
                }
                None => "gs", // default to GS if game state not yet initialized
            }
        };

        // Transform "lib/bounty" → "lib/gs/bounty"
        let rest = &module_name[prefix.len()..];
        // Don't double-transform if already game-qualified (lib/gs/X or lib/dr/X)
        if rest.starts_with("gs/") || rest.starts_with("dr/") {
            return Ok(mlua::Value::String(lua.create_string(
                format!("\n\tgame-searcher: already game-qualified: '{module_name}'")
            )?));
        }

        let game_module = format!("{}{}/{}", prefix, game_sub, rest);
        let dir = scripts_dir.lock().unwrap().clone();
        let game_path = format!("{}/{}.lua", dir, game_module.replace('.', "/"));

        if std::path::Path::new(&game_path).exists() {
            // Load the game-specific file
            let code = std::fs::read_to_string(&game_path)
                .map_err(|e| mlua::Error::RuntimeError(format!("failed to read {game_path}: {e}")))?;
            let func = lua.load(code).set_name(&game_module).into_function()?;
            Ok(mlua::Value::Function(func))
        } else {
            // Not found — let other searchers try
            Ok(mlua::Value::String(lua.create_string(
                format!("\n\tgame-searcher: no game-specific file at '{game_path}'")
            )?))
        }
    })?;

    // Insert our searcher at position 1 (before the default file searcher at position 2)
    let package: mlua::Table = engine.lua.globals().get("package")?;
    let searchers: mlua::Table = package.get("searchers")?;
    // Shift existing searchers up by one
    let len = searchers.len()? as i64;
    for i in (1..=len).rev() {
        let v: mlua::Value = searchers.get(i)?;
        searchers.set(i + 1, v)?;
    }
    searchers.set(1, searcher)?;

    Ok(())
}
