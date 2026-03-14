use mlua::prelude::*;
use std::path::PathBuf;
use std::time::UNIX_EPOCH;

use crate::script_engine::ScriptEngine;

/// Sandbox check for paths that may not exist yet (write, mkdir).
/// Checks component-level for traversal, then verifies the nearest
/// existing ancestor canonicalizes within the scripts directory.
fn resolve_sandboxed(scripts_dir: &str, path: &str) -> Result<PathBuf, String> {
    if path.starts_with('/') {
        return Err("absolute paths not allowed".to_string());
    }
    let base = PathBuf::from(scripts_dir);
    let joined = base.join(path);
    // Reject .. components
    for component in joined.components() {
        if let std::path::Component::ParentDir = component {
            return Err("path escapes scripts directory".to_string());
        }
    }
    // Verify nearest existing ancestor is within the sandbox
    // (prevents symlink-based escapes)
    let canonical_base = base
        .canonicalize()
        .map_err(|e| format!("scripts dir error: {e}"))?;
    let mut check = joined.clone();
    loop {
        if check.exists() {
            let canonical = check
                .canonicalize()
                .map_err(|e| format!("path error: {e}"))?;
            if !canonical.starts_with(&canonical_base) {
                return Err("path escapes scripts directory".to_string());
            }
            break;
        }
        if !check.pop() {
            break;
        }
    }
    Ok(joined)
}

/// Sandbox check for paths that must already exist (read, remove, list, mtime).
/// Uses full canonicalization including symlink resolution.
fn resolve_sandboxed_existing(scripts_dir: &str, path: &str) -> Result<PathBuf, String> {
    if path.starts_with('/') {
        return Err("absolute paths not allowed".to_string());
    }
    let base = PathBuf::from(scripts_dir);
    let joined = base.join(path);
    let canonical_base = base
        .canonicalize()
        .map_err(|e| format!("scripts dir error: {e}"))?;
    let canonical = joined
        .canonicalize()
        .map_err(|e| format!("path error: {e}"))?;
    if !canonical.starts_with(&canonical_base) {
        return Err("path escapes scripts directory".to_string());
    }
    Ok(canonical)
}

pub fn register(engine: &ScriptEngine) -> LuaResult<()> {
    let lua = &engine.lua;
    let globals = lua.globals();

    let file_table = lua.create_table()?;

    // File.read(path) -> string or nil, error
    let dir = engine.scripts_dir.clone();
    file_table.set(
        "read",
        lua.create_function(move |_, path: String| {
            let d = dir.lock().unwrap().clone();
            match resolve_sandboxed_existing(&d, &path) {
                Ok(full) => match std::fs::read_to_string(&full) {
                    Ok(s) => Ok((Some(s), None::<String>)),
                    Err(e) => Ok((None, Some(format!("{e}")))),
                },
                Err(e) => Ok((None, Some(e))),
            }
        })?,
    )?;

    // File.write(path, content) -> true or nil, error
    let dir = engine.scripts_dir.clone();
    file_table.set(
        "write",
        lua.create_function(move |_, (path, content): (String, String)| {
            let d = dir.lock().unwrap().clone();
            match resolve_sandboxed(&d, &path) {
                Ok(full) => {
                    // Ensure parent directory exists
                    if let Some(parent) = full.parent() {
                        let _ = std::fs::create_dir_all(parent);
                    }
                    match std::fs::write(&full, content) {
                        Ok(()) => Ok((Some(true), None::<String>)),
                        Err(e) => Ok((None, Some(format!("{e}")))),
                    }
                }
                Err(e) => Ok((None, Some(e))),
            }
        })?,
    )?;

    // File.exists(path) -> bool
    let dir = engine.scripts_dir.clone();
    file_table.set(
        "exists",
        lua.create_function(move |_, path: String| {
            let d = dir.lock().unwrap().clone();
            match resolve_sandboxed(&d, &path) {
                Ok(full) => Ok(full.exists()),
                Err(_) => Ok(false),
            }
        })?,
    )?;

    // File.mkdir(path) -> true or nil, error
    let dir = engine.scripts_dir.clone();
    file_table.set(
        "mkdir",
        lua.create_function(move |_, path: String| {
            let d = dir.lock().unwrap().clone();
            match resolve_sandboxed(&d, &path) {
                Ok(full) => match std::fs::create_dir_all(&full) {
                    Ok(()) => Ok((Some(true), None::<String>)),
                    Err(e) => Ok((None, Some(format!("{e}")))),
                },
                Err(e) => Ok((None, Some(e))),
            }
        })?,
    )?;

    // File.remove(path) -> true or nil, error
    let dir = engine.scripts_dir.clone();
    file_table.set(
        "remove",
        lua.create_function(move |_, path: String| {
            let d = dir.lock().unwrap().clone();
            match resolve_sandboxed_existing(&d, &path) {
                Ok(full) => {
                    let result = if full.is_dir() {
                        std::fs::remove_dir_all(&full)
                    } else {
                        std::fs::remove_file(&full)
                    };
                    match result {
                        Ok(()) => Ok((Some(true), None::<String>)),
                        Err(e) => Ok((None, Some(format!("{e}")))),
                    }
                }
                Err(e) => Ok((None, Some(e))),
            }
        })?,
    )?;

    // File.list(path) -> table or nil, error
    let dir = engine.scripts_dir.clone();
    file_table.set(
        "list",
        lua.create_function(move |lua, path: String| {
            let d = dir.lock().unwrap().clone();
            match resolve_sandboxed_existing(&d, &path) {
                Ok(full) => match std::fs::read_dir(&full) {
                    Ok(entries) => {
                        let t = lua.create_table()?;
                        let mut i = 1;
                        for entry in entries.flatten() {
                            let mut name = entry.file_name().to_string_lossy().to_string();
                            if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                                name.push('/');
                            }
                            t.raw_set(i, name)?;
                            i += 1;
                        }
                        Ok((Some(t), None::<String>))
                    }
                    Err(e) => Ok((None, Some(format!("{e}")))),
                },
                Err(e) => Ok((None, Some(e))),
            }
        })?,
    )?;

    // File.is_dir(path) -> bool
    let dir = engine.scripts_dir.clone();
    file_table.set(
        "is_dir",
        lua.create_function(move |_, path: String| {
            let d = dir.lock().unwrap().clone();
            match resolve_sandboxed(&d, &path) {
                Ok(full) => Ok(full.is_dir()),
                Err(_) => Ok(false),
            }
        })?,
    )?;

    // File.mtime(path) -> integer (unix timestamp) or nil, error
    let dir = engine.scripts_dir.clone();
    file_table.set(
        "mtime",
        lua.create_function(move |_, path: String| {
            let d = dir.lock().unwrap().clone();
            match resolve_sandboxed_existing(&d, &path) {
                Ok(full) => match std::fs::metadata(&full) {
                    Ok(meta) => match meta.modified() {
                        Ok(time) => {
                            let secs = time
                                .duration_since(UNIX_EPOCH)
                                .unwrap_or_default()
                                .as_secs();
                            Ok((Some(secs as i64), None::<String>))
                        }
                        Err(e) => Ok((None, Some(format!("{e}")))),
                    },
                    Err(e) => Ok((None, Some(format!("{e}")))),
                },
                Err(e) => Ok((None, Some(e))),
            }
        })?,
    )?;

    // File.replace(src, dst) -> true or nil, error
    // src must be sandboxed and exist. dst must be sandboxed OR equal the engine binary path.
    let dir = engine.scripts_dir.clone();
    file_table.set(
        "replace",
        lua.create_function(move |_, (src, dst): (String, String)| {
            let d = dir.lock().unwrap().clone();
            // src must be an existing sandboxed path
            let src_path = match resolve_sandboxed_existing(&d, &src) {
                Ok(p) => p,
                Err(e) => return Ok((None::<bool>, Some(e))),
            };
            // dst: sandboxed or equals the engine binary path
            let dst_path = if dst.starts_with('/') {
                // Absolute path — only allowed if it matches the engine binary
                let engine_path = match std::env::current_exe() {
                    Ok(p) => p,
                    Err(e) => return Ok((None, Some(format!("could not determine engine path: {e}")))),
                };
                let dst_canonical = std::path::PathBuf::from(&dst);
                if dst_canonical != engine_path {
                    return Ok((None, Some("absolute dst must equal engine binary path".to_string())));
                }
                dst_canonical
            } else {
                match resolve_sandboxed(&d, &dst) {
                    Ok(p) => p,
                    Err(e) => return Ok((None, Some(e))),
                }
            };
            // Ensure dst parent directory exists
            if let Some(parent) = dst_path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            match std::fs::rename(&src_path, &dst_path) {
                Ok(()) => Ok((Some(true), None)),
                Err(e) => Ok((None, Some(format!("{e}")))),
            }
        })?,
    )?;

    globals.set("File", file_table)?;
    Ok(())
}
