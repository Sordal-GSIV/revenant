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
    pub script_args: Arc<Mutex<HashMap<String, Vec<String>>>>,
    pub downstream_hooks: Arc<Mutex<crate::hook_chain::HookChain>>,
    pub upstream_hooks: Arc<Mutex<crate::hook_chain::HookChain>>,
    pub db: Arc<Mutex<Option<crate::db::Db>>>,
    pub character: Arc<Mutex<String>>,
    pub game: Arc<Mutex<String>>,
    /// Optional hook called when a script exits with an error. Used in tests.
    pub script_error_hook: Arc<Mutex<Option<Box<dyn Fn(String, String) + Send + Sync>>>>,
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
            db: Arc::new(Mutex::new(None)),
            character: Arc::new(Mutex::new(String::new())),
            game: Arc::new(Mutex::new("GS3".to_string())),
            script_error_hook: Arc::new(Mutex::new(None)),
        }
    }

    /// Register a callback invoked when a script exits with an error.
    /// Signature: `fn(script_name: String, error_message: String)`.
    /// Primarily used in tests to surface Lua assertion failures.
    pub fn set_script_error_hook<F: Fn(String, String) + Send + Sync + 'static>(&self, f: F) {
        *self.script_error_hook.lock().unwrap() = Some(Box::new(f));
    }

    pub fn set_upstream_sink<F: Fn(String) + Send + Sync + 'static>(&self, f: F) {
        *self.upstream_sink.lock().unwrap() = Some(Box::new(f));
    }

    pub fn set_downstream_channel(&self, tx: tokio::sync::broadcast::Sender<Arc<Vec<u8>>>) {
        *self.downstream_tx.lock().unwrap() = Some(tx);
    }

    pub fn set_game_state(&self, gs: Arc<RwLock<crate::game_state::GameState>>) {
        let mut lock = self.game_state.lock().unwrap();
        if lock.is_some() {
            tracing::warn!("set_game_state called but game_state is already set — single-client constraint violated");
        }
        *lock = Some(gs);
    }

    /// Send a message directly to the client output stream.
    /// Stub: prints to stdout until the client_tx channel is wired (Task 1).
    pub fn respond(&self, msg: &str) {
        println!("{msg}");
    }

    /// Pause all running scripts. (Implemented in Task 4.)
    pub fn pause_all(&self) {}

    /// Unpause all running scripts. (Implemented in Task 4.)
    pub fn unpause_all(&self) {}

    pub fn set_scripts_dir(&self, dir: &str) {
        *self.scripts_dir.lock().unwrap() = dir.to_string();
    }

    pub fn set_db(&self, db: crate::db::Db, character: &str, game: &str) {
        *self.db.lock().unwrap() = Some(db);
        *self.character.lock().unwrap() = character.to_string();
        *self.game.lock().unwrap() = game.to_string();
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

    /// Kill all running scripts.
    pub async fn kill_all(&self) {
        let handles: Vec<_> = self.running.lock().unwrap().drain().collect();
        for (_name, handle) in handles {
            handle.abort();
        }
    }

    /// Pause a named script. (Implemented in Task 4.)
    pub fn pause_script(&self, _name: &str) {}

    /// Unpause a named script. (Implemented in Task 4.)
    pub fn unpause_script(&self, _name: &str) {}

    /// Launch a named script from a file path as a tokio task.
    /// `args` follows Lich5 convention: args[0] = full arg string, args[1..] = individual tokens.
    pub fn start_script(&self, name: &str, path: &str, args: Vec<String>) -> Result<()> {
        let code = std::fs::read_to_string(path)?;
        let lua = self.lua.clone();
        let script_name = name.to_string();

        // Store args in Rust-side map
        self.script_args.lock().unwrap().insert(name.to_string(), args.clone());

        // Inject globals before launch
        {
            let globals = lua.globals();
            globals.set("_REVENANT_SCRIPT", name)
                .map_err(|e| anyhow::anyhow!("lua globals: {e}"))?;
            let args_table = lua.create_table()
                .map_err(|e| anyhow::anyhow!("lua table: {e}"))?;
            for (i, a) in args.iter().enumerate() {
                args_table.raw_set(i as i64, a.as_str())
                    .map_err(|e| anyhow::anyhow!("{e}"))?;
            }
            let all_args: mlua::Table = globals.get("_REVENANT_SCRIPT_ARGS")
                .unwrap_or_else(|_| lua.create_table().unwrap());
            all_args.set(name, args_table)
                .map_err(|e| anyhow::anyhow!("{e}"))?;
            globals.set("_REVENANT_SCRIPT_ARGS", all_args)
                .map_err(|e| anyhow::anyhow!("{e}"))?;
        }

        let error_hook = self.script_error_hook.clone();
        let handle = tokio::spawn(async move {
            let result: LuaResult<()> = async {
                let func: LuaFunction = lua.load(&code).set_name(&script_name).into_function()?;
                let thread = lua.create_thread(func)?;
                thread.into_async::<mlua::MultiValue>(mlua::MultiValue::new()).await?;
                Ok(())
            }.await;
            if let Err(e) = result {
                let msg = e.to_string();
                tracing::error!("[script:{script_name}] error: {msg}");
                if let Some(hook) = error_hook.lock().unwrap().as_ref() {
                    hook(script_name, msg);
                }
            }
        });

        self.running.lock().unwrap().insert(name.to_string(), handle);
        Ok(())
    }
}

impl Default for ScriptEngine {
    fn default() -> Self { Self::new() }
}
