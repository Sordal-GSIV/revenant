use mlua::prelude::*;
use md5::Md5;
use sha1::Sha1;
use sha2::{Digest, Sha256};
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use std::path::PathBuf;

use crate::script_engine::ScriptEngine;

fn resolve_sandboxed_path(scripts_dir: &str, path: &str) -> LuaResult<PathBuf> {
    let base = PathBuf::from(scripts_dir);
    let resolved = base.join(path);
    let canonical_base = base
        .canonicalize()
        .map_err(|e| LuaError::runtime(format!("scripts dir error: {e}")))?;
    let canonical = resolved
        .canonicalize()
        .map_err(|e| LuaError::runtime(format!("path error: {e}")))?;
    if !canonical.starts_with(&canonical_base) {
        return Err(LuaError::runtime("path escapes scripts directory"));
    }
    Ok(canonical)
}

pub fn register(engine: &ScriptEngine) -> LuaResult<()> {
    let lua = &engine.lua;
    let globals = lua.globals();

    let crypto_table = lua.create_table()?;

    crypto_table.set(
        "sha256",
        lua.create_function(|_, content: String| {
            let mut hasher = Sha256::new();
            hasher.update(content.as_bytes());
            Ok(format!("{:x}", hasher.finalize()))
        })?,
    )?;

    let scripts_dir = engine.scripts_dir.clone();
    crypto_table.set(
        "sha256_file",
        lua.create_function(move |_, path: String| {
            let dir = scripts_dir.lock().unwrap().clone();
            match resolve_sandboxed_path(&dir, &path) {
                Ok(full_path) => match std::fs::read(&full_path) {
                    Ok(bytes) => {
                        let mut hasher = Sha256::new();
                        hasher.update(&bytes);
                        Ok((Some(format!("{:x}", hasher.finalize())), None::<String>))
                    }
                    Err(e) => Ok((None, Some(format!("read error: {e}")))),
                },
                Err(e) => Ok((None, Some(e.to_string()))),
            }
        })?,
    )?;

    // md5(content) -> hex string
    crypto_table.set(
        "md5",
        lua.create_function(|_, content: String| {
            let mut hasher = Md5::new();
            hasher.update(content.as_bytes());
            Ok(format!("{:x}", hasher.finalize()))
        })?,
    )?;

    // sha1(content) -> hex string
    crypto_table.set(
        "sha1",
        lua.create_function(|_, content: String| {
            let mut hasher = Sha1::new();
            hasher.update(content.as_bytes());
            Ok(format!("{:x}", hasher.finalize()))
        })?,
    )?;

    // sha1_base64(content) -> base64 string
    crypto_table.set(
        "sha1_base64",
        lua.create_function(|_, content: String| {
            let mut hasher = Sha1::new();
            hasher.update(content.as_bytes());
            Ok(BASE64.encode(hasher.finalize()))
        })?,
    )?;

    // sha1_file(path) -> hex string or nil, err
    let scripts_dir2 = engine.scripts_dir.clone();
    crypto_table.set(
        "sha1_file",
        lua.create_function(move |_, path: String| {
            let dir = scripts_dir2.lock().unwrap().clone();
            match resolve_sandboxed_path(&dir, &path) {
                Ok(full_path) => match std::fs::read(&full_path) {
                    Ok(bytes) => {
                        let mut hasher = Sha1::new();
                        hasher.update(&bytes);
                        Ok((Some(format!("{:x}", hasher.finalize())), None::<String>))
                    }
                    Err(e) => Ok((None, Some(format!("read error: {e}")))),
                },
                Err(e) => Ok((None, Some(e.to_string()))),
            }
        })?,
    )?;

    // sha1_file_base64(path) -> base64 string or nil, err
    let scripts_dir3 = engine.scripts_dir.clone();
    crypto_table.set(
        "sha1_file_base64",
        lua.create_function(move |_, path: String| {
            let dir = scripts_dir3.lock().unwrap().clone();
            match resolve_sandboxed_path(&dir, &path) {
                Ok(full_path) => match std::fs::read(&full_path) {
                    Ok(bytes) => {
                        let mut hasher = Sha1::new();
                        hasher.update(&bytes);
                        Ok((Some(BASE64.encode(hasher.finalize())), None::<String>))
                    }
                    Err(e) => Ok((None, Some(format!("read error: {e}")))),
                },
                Err(e) => Ok((None, Some(e.to_string()))),
            }
        })?,
    )?;

    globals.set("Crypto", crypto_table)?;
    Ok(())
}
