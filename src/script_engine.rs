use anyhow::Result;
use crate::game_obj::GameObjRegistry;
use crate::map::MapData;
use mlua::prelude::*;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, RwLock};
use tokio::task::JoinHandle;

#[allow(clippy::type_complexity)]
pub struct ScriptEngine {
    pub lua: Arc<Lua>,
    pub upstream_sink: Arc<Mutex<Option<Box<dyn Fn(String) + Send + Sync>>>>,
    pub downstream_tx: Arc<Mutex<Option<tokio::sync::broadcast::Sender<Arc<Vec<u8>>>>>>,
    pub respond_sink: Arc<Mutex<Option<Box<dyn Fn(String) + Send + Sync>>>>,
    pub game_state: Arc<Mutex<Option<Arc<RwLock<crate::game_state::GameState>>>>>,
    pub game_objs: Arc<Mutex<Option<Arc<Mutex<GameObjRegistry>>>>>,
    pub map_data: Arc<RwLock<Option<MapData>>>,
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
    /// Set of script names that are currently paused.
    pub paused: Arc<Mutex<std::collections::HashSet<String>>>,
    /// Ring-buffer of the last 500 respond() messages, for the monitor window.
    pub respond_log: Arc<Mutex<std::collections::VecDeque<String>>>,
    /// Ring-buffer of the last 2000 lines of game text (XmlEvent::Text), for the monitor window.
    pub game_log: Arc<Mutex<std::collections::VecDeque<String>>>,
    /// Maps raw Lua thread pointer (as usize) → script name for per-coroutine identity.
    /// Entries are inserted when a thread starts and removed when it completes
    /// (but NOT when aborted via kill_script/kill_all).
    pub thread_names: Arc<Mutex<HashMap<usize, String>>>,
    /// Per-script line buffer senders. Used by feeder tasks and send_to_script().
    pub script_lines_tx: Arc<Mutex<HashMap<String, tokio::sync::mpsc::UnboundedSender<String>>>>,
    /// Per-script line buffer receivers. Key = script name.
    pub script_lines_rx: Arc<Mutex<HashMap<String, Arc<tokio::sync::Mutex<tokio::sync::mpsc::UnboundedReceiver<String>>>>>>,
    /// Per-script at-exit callback registry keys (Lua registry references). LIFO order.
    pub at_exit_hooks: Arc<Mutex<HashMap<String, Vec<mlua::RegistryKey>>>>,
    /// Scripts protected from kill_all.
    pub no_kill_all: Arc<Mutex<std::collections::HashSet<String>>>,
    /// Scripts protected from pause_all.
    pub no_pause_all: Arc<Mutex<std::collections::HashSet<String>>>,
}

impl ScriptEngine {
    pub fn new() -> Self {
        Self {
            lua: Arc::new(Lua::new()),
            upstream_sink: Arc::new(Mutex::new(None)),
            downstream_tx: Arc::new(Mutex::new(None)),
            respond_sink: Arc::new(Mutex::new(None)),
            game_state: Arc::new(Mutex::new(None)),
            game_objs: Arc::new(Mutex::new(None)),
            map_data: Arc::new(RwLock::new(None)),
            scripts_dir: Arc::new(Mutex::new("../scripts".to_string())),
            running: Arc::new(Mutex::new(HashMap::new())),
            script_args: Arc::new(Mutex::new(HashMap::new())),
            downstream_hooks: Arc::new(Mutex::new(crate::hook_chain::HookChain::new())),
            upstream_hooks: Arc::new(Mutex::new(crate::hook_chain::HookChain::new())),
            db: Arc::new(Mutex::new(None)),
            character: Arc::new(Mutex::new(String::new())),
            game: Arc::new(Mutex::new("GS3".to_string())),
            script_error_hook: Arc::new(Mutex::new(None)),
            paused: Arc::new(Mutex::new(std::collections::HashSet::new())),
            respond_log: Arc::new(Mutex::new(std::collections::VecDeque::with_capacity(500))),
            game_log: Arc::new(Mutex::new(std::collections::VecDeque::with_capacity(2000))),
            thread_names: Arc::new(Mutex::new(HashMap::new())),
            script_lines_tx: Arc::new(Mutex::new(HashMap::new())),
            script_lines_rx: Arc::new(Mutex::new(HashMap::new())),
            at_exit_hooks: Arc::new(Mutex::new(HashMap::new())),
            no_kill_all: Arc::new(Mutex::new(std::collections::HashSet::new())),
            no_pause_all: Arc::new(Mutex::new(std::collections::HashSet::new())),
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

    pub fn set_respond_sink<F: Fn(String) + Send + Sync + 'static>(&self, f: F) {
        *self.respond_sink.lock().unwrap() = Some(Box::new(f));
    }

    pub fn set_game_objs(&self, go: Arc<Mutex<GameObjRegistry>>) {
        *self.game_objs.lock().unwrap() = Some(go);
    }

    pub fn clear_game_objs(&self) {
        *self.game_objs.lock().unwrap() = None;
    }

    pub fn set_game_state(&self, gs: Arc<RwLock<crate::game_state::GameState>>) {
        let mut lock = self.game_state.lock().unwrap();
        if lock.is_some() {
            tracing::warn!("set_game_state called but game_state is already set — single-client constraint violated");
        }
        *lock = Some(gs);
    }

    /// Send a message directly to the client output stream.
    pub fn respond(&self, msg: &str) {
        {
            let mut log = self.respond_log.lock().unwrap();
            if log.len() >= 500 { log.pop_front(); }
            log.push_back(msg.to_string());
        }
        if let Some(f) = self.respond_sink.lock().unwrap().as_ref() {
            f(format!("<output class=\"mono\">{msg}</output>\n"));
        } else {
            println!("[respond] {msg}");
        }
    }

    /// Pause all running scripts (respects no_pause_all protection).
    pub fn pause_all(&self) {
        let protected = self.no_pause_all.lock().unwrap().clone();
        let names: Vec<String> = self.running.lock().unwrap().keys()
            .filter(|n| !protected.contains(*n))
            .cloned().collect();
        let mut p = self.paused.lock().unwrap();
        for n in names { p.insert(n); }
    }

    /// Unpause all running scripts.
    pub fn unpause_all(&self) {
        self.paused.lock().unwrap().clear();
    }

    pub fn set_scripts_dir(&self, dir: &str) {
        *self.scripts_dir.lock().unwrap() = dir.to_string();
    }

    pub fn set_db(&self, db: crate::db::Db, character: &str, game: &str) {
        *self.db.lock().unwrap() = Some(db);
        *self.character.lock().unwrap() = character.to_string();
        *self.game.lock().unwrap() = game.to_string();
    }

    pub fn load_map(&self, path: &str) -> Result<()> {
        let data = MapData::from_file(path)?;
        *self.map_data.write().unwrap_or_else(|e| e.into_inner()) = Some(data);
        Ok(())
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
        self.script_lines_tx.lock().unwrap().remove(name);
        self.script_lines_rx.lock().unwrap().remove(name);
        self.at_exit_hooks.lock().unwrap().remove(name);
    }

    /// Kill all running scripts (respects no_kill_all protection).
    pub async fn kill_all(&self) {
        let protected = self.no_kill_all.lock().unwrap().clone();
        let mut to_kill: Vec<(String, JoinHandle<()>)> = Vec::new();
        let mut to_keep: HashMap<String, JoinHandle<()>> = HashMap::new();
        {
            let mut running = self.running.lock().unwrap();
            for (name, handle) in running.drain() {
                if protected.contains(&name) {
                    to_keep.insert(name, handle);
                } else {
                    to_kill.push((name, handle));
                }
            }
            *running = to_keep;
        }
        {
            let mut tx_map = self.script_lines_tx.lock().unwrap();
            let mut rx_map = self.script_lines_rx.lock().unwrap();
            let mut hooks_map = self.at_exit_hooks.lock().unwrap();
            for (name, _) in &to_kill {
                tx_map.remove(name);
                rx_map.remove(name);
                hooks_map.remove(name);
            }
        }
        for (_name, handle) in to_kill {
            handle.abort();
        }
    }

    /// Pause a named script.
    pub fn pause_script(&self, name: &str) {
        self.paused.lock().unwrap().insert(name.to_string());
    }

    /// Unpause a named script.
    pub fn unpause_script(&self, name: &str) {
        self.paused.lock().unwrap().remove(name);
    }

    /// Launch a named script from a file path as a tokio task.
    /// `args` follows Lich5 convention: args[0] = full arg string, args[1..] = individual tokens.
    pub fn start_script(&self, name: &str, path: &str, args: Vec<String>) -> Result<()> {
        let raw_code = std::fs::read_to_string(path)?;

        // If this is a package script, wrap code with scoped package.path
        let code = if path.ends_with("/init.lua") {
            if let Some(pkg_dir) = std::path::Path::new(path).parent() {
                let pkg_dir_str = pkg_dir.to_string_lossy();
                let scripts_dir = self.scripts_dir.lock().unwrap().clone();
                let wrapper = format!(
                    "do\nlocal _saved_path = package.path\npackage.path = \"{}/?.lua;{}/?.lua;\" .. package.path\nlocal _ok, _err = pcall(function()\n",
                    pkg_dir_str, scripts_dir
                );
                wrapper + &raw_code + "\nend)\npackage.path = _saved_path\nif not _ok then error(_err) end\nend"
            } else {
                raw_code
            }
        } else {
            raw_code
        };

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

        // Create per-script line buffer (MPSC channel)
        let (lines_tx, lines_rx) = tokio::sync::mpsc::unbounded_channel::<String>();
        self.script_lines_tx.lock().unwrap().insert(name.to_string(), lines_tx);
        self.script_lines_rx.lock().unwrap().insert(
            name.to_string(),
            Arc::new(tokio::sync::Mutex::new(lines_rx)),
        );

        // Spawn feeder task: broadcast channel → per-script MPSC buffer
        if let Some(broadcast_tx) = self.downstream_tx.lock().unwrap().as_ref() {
            let mut broadcast_rx = broadcast_tx.subscribe();
            let feeder_tx = self.script_lines_tx.lock().unwrap().get(name).unwrap().clone();
            let feeder_name = name.to_string();
            tokio::spawn(async move {
                loop {
                    match broadcast_rx.recv().await {
                        Ok(bytes) => {
                            let text = String::from_utf8_lossy(&bytes);
                            for line in text.lines() {
                                let trimmed = line.trim_end();
                                if !trimmed.is_empty() {
                                    if feeder_tx.send(trimmed.to_string()).is_err() {
                                        return; // receiver dropped, script exited
                                    }
                                }
                            }
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => return,
                    }
                }
            });
            let _ = feeder_name; // suppress unused warning
        }

        let error_hook = self.script_error_hook.clone();
        let thread_names = self.thread_names.clone();
        let script_lines_tx_clone = self.script_lines_tx.clone();
        let script_lines_rx_clone = self.script_lines_rx.clone();
        let at_exit_hooks_clone = self.at_exit_hooks.clone();
        let handle = tokio::spawn(async move {
            let result: LuaResult<()> = async {
                let func: LuaFunction = lua.load(&code).set_name(&script_name).into_function()?;
                let thread = lua.create_thread(func)?;
                // Register per-coroutine identity: thread pointer → script name
                let ptr = thread.to_pointer() as usize;
                thread_names.lock().unwrap().insert(ptr, script_name.clone());
                let r = thread.into_async::<mlua::MultiValue>(mlua::MultiValue::new()).await;
                thread_names.lock().unwrap().remove(&ptr);
                r?;
                Ok(())
            }.await;
            // Run at-exit hooks (LIFO) — runs on both success and error
            let hooks = at_exit_hooks_clone.lock().unwrap().remove(&script_name);
            if let Some(hook_keys) = hooks {
                for key in hook_keys.into_iter().rev() {
                    if let Ok(func) = lua.registry_value::<LuaFunction>(&key) {
                        if let Err(e) = func.call_async::<()>(()).await {
                            tracing::warn!("[script:{script_name}] at_exit hook error: {e}");
                        }
                    }
                    let _ = lua.remove_registry_value(key);
                }
            }
            if let Err(e) = result {
                let msg = e.to_string();
                tracing::error!("[script:{script_name}] error: {msg}");
                if let Some(hook) = error_hook.lock().unwrap().as_ref() {
                    hook(script_name.clone(), msg);
                }
            }
            // Clean up args table to avoid unbounded growth
            if let Ok(globals) = lua.globals().get::<mlua::Table>("_REVENANT_SCRIPT_ARGS") {
                let _ = globals.raw_remove(script_name.as_str());
            }
            // Clean up line buffer entries
            script_lines_tx_clone.lock().unwrap().remove(&script_name);
            script_lines_rx_clone.lock().unwrap().remove(&script_name);
        });

        self.running.lock().unwrap().insert(name.to_string(), handle);
        Ok(())
    }
}

impl Default for ScriptEngine {
    fn default() -> Self { Self::new() }
}
