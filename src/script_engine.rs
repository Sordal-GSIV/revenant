use anyhow::Result;
use mlua::prelude::*;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, RwLock};
use tokio::task::JoinHandle;

pub struct ScriptEngine {
    pub lua: Arc<Lua>,
    pub upstream_sink: Arc<Mutex<Option<Box<dyn Fn(String) + Send + Sync>>>>,
    pub downstream_tx: Arc<Mutex<Option<tokio::sync::broadcast::Sender<Arc<Vec<u8>>>>>>,
    pub game_state: Arc<Mutex<Option<Arc<RwLock<crate::game_state::GameState>>>>>,
    pub scripts_dir: Arc<Mutex<String>>,
    pub running: Arc<Mutex<HashMap<String, JoinHandle<()>>>>,
    pub script_args: Arc<Mutex<HashMap<String, String>>>,
    pub downstream_hooks: Arc<Mutex<crate::hook_chain::HookChain>>,
    pub upstream_hooks: Arc<Mutex<crate::hook_chain::HookChain>>,
}

impl ScriptEngine {
    pub fn new() -> Self {
        Self {
            lua: Arc::new(Lua::new()),
            upstream_sink: Arc::new(Mutex::new(None)),
            downstream_tx: Arc::new(Mutex::new(None)),
            game_state: Arc::new(Mutex::new(None)),
            scripts_dir: Arc::new(Mutex::new("../scripts".to_string())),
            running: Arc::new(Mutex::new(HashMap::new())),
            script_args: Arc::new(Mutex::new(HashMap::new())),
            downstream_hooks: Arc::new(Mutex::new(crate::hook_chain::HookChain::new())),
            upstream_hooks: Arc::new(Mutex::new(crate::hook_chain::HookChain::new())),
        }
    }

    pub fn set_upstream_sink<F: Fn(String) + Send + Sync + 'static>(&self, f: F) {
        *self.upstream_sink.lock().unwrap() = Some(Box::new(f));
    }

    pub fn set_downstream_channel(&self, tx: tokio::sync::broadcast::Sender<Arc<Vec<u8>>>) {
        *self.downstream_tx.lock().unwrap() = Some(tx);
    }

    pub fn set_game_state(&self, gs: Arc<RwLock<crate::game_state::GameState>>) {
        *self.game_state.lock().unwrap() = Some(gs);
    }

    pub fn set_scripts_dir(&self, dir: &str) {
        *self.scripts_dir.lock().unwrap() = dir.to_string();
    }

    /// Evaluate Lua code string. Used for tests and REPL.
    pub async fn eval_lua(&self, code: &str) -> Result<()> {
        self.lua.load(code).into_function()?.call_async::<()>(()).await?;
        Ok(())
    }

    /// Install all Lua globals. Call after setting upstream_sink, game_state, etc.
    pub fn install_lua_api(&self) -> Result<()> {
        crate::lua_api::register_all(self)
    }

    pub fn is_running(&self, name: &str) -> bool {
        self.running.lock().unwrap()
            .get(name).map(|h| !h.is_finished()).unwrap_or(false)
    }

    pub async fn kill_script(&self, name: &str) {
        let handle = self.running.lock().unwrap().remove(name);
        if let Some(h) = handle { h.abort(); }
    }

    /// Launch a named script from a file path as a tokio task.
    pub fn start_script(&self, name: &str, path: &str) -> Result<()> {
        let code = std::fs::read_to_string(path)?;
        let lua = self.lua.clone();
        let script_name = name.to_string();

        let handle = tokio::spawn(async move {
            let result: LuaResult<()> = async {
                let func: LuaFunction = lua.load(&code).set_name(&script_name).into_function()?;
                let thread = lua.create_thread(func)?;
                thread.into_async::<mlua::MultiValue>(mlua::MultiValue::new()).await?;
                Ok(())
            }.await;
            if let Err(e) = result {
                tracing::error!("[script:{script_name}] error: {e}");
            }
        });

        self.running.lock().unwrap().insert(name.to_string(), handle);
        Ok(())
    }
}

impl Default for ScriptEngine {
    fn default() -> Self { Self::new() }
}
