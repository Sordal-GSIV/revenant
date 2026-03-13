use mlua::prelude::*;

type SyncFn = Box<dyn Fn(&str) -> Option<String> + Send + Sync>;

pub enum HookEntry {
    Sync(SyncFn),
    Lua(LuaRegistryKey),
}

pub struct HookChain {
    hooks: Vec<(String, HookEntry)>,
}

impl HookChain {
    pub fn new() -> Self { Self { hooks: Vec::new() } }

    pub fn add_sync<F>(&mut self, name: &str, f: F)
    where F: Fn(&str) -> Option<String> + Send + Sync + 'static {
        self.hooks.retain(|(n, _)| n != name);
        self.hooks.push((name.to_string(), HookEntry::Sync(Box::new(f))));
    }

    pub fn add_lua(&mut self, name: String, key: LuaRegistryKey) {
        self.hooks.retain(|(n, _)| n != &name);
        self.hooks.push((name, HookEntry::Lua(key)));
    }

    pub fn remove(&mut self, name: &str) {
        self.hooks.retain(|(n, _)| n != name);
    }

    pub fn hook_names(&self) -> Vec<String> {
        self.hooks.iter().map(|(n, _)| n.clone()).collect()
    }

    /// Process through sync hooks only (for testing and sync contexts).
    pub fn process_sync(&self, line: &str) -> Option<String> {
        let mut current = line.to_string();
        for (_, entry) in &self.hooks {
            match entry {
                HookEntry::Sync(f) => match f(&current) {
                    Some(s) => current = s,
                    None => return None,
                },
                HookEntry::Lua(_) => {} // Lua hooks require process_with_lua
            }
        }
        Some(current)
    }

    /// Process through all hooks including Lua. Caller must NOT hold any
    /// mutex on this HookChain — Lua callbacks may call add/remove.
    pub fn process_with_lua(&self, lua: &Lua, line: &str) -> LuaResult<Option<String>> {
        let mut current = line.to_string();
        for (_, entry) in &self.hooks {
            match entry {
                HookEntry::Sync(f) => match f(&current) {
                    Some(s) => current = s,
                    None => return Ok(None),
                },
                HookEntry::Lua(key) => {
                    let func: LuaFunction = lua.registry_value(key)?;
                    match func.call::<LuaValue>(current.clone())? {
                        LuaValue::Nil => return Ok(None),
                        LuaValue::String(s) => current = s.to_str()?.to_string(),
                        _ => {}
                    }
                }
            }
        }
        Ok(Some(current))
    }
}

impl Default for HookChain {
    fn default() -> Self { Self::new() }
}
