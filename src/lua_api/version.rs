use mlua::prelude::*;
use semver::{Version, VersionReq};

use crate::script_engine::ScriptEngine;

/// Normalize version strings that may be missing patch (e.g., "1.0" → "1.0.0")
fn normalize_version(s: &str) -> String {
    let parts: Vec<&str> = s.splitn(2, '-').collect();
    let nums: Vec<&str> = parts[0].split('.').collect();
    let base = match nums.len() {
        1 => format!("{}.0.0", nums[0]),
        2 => format!("{}.{}.0", nums[0], nums[1]),
        _ => parts[0].to_string(),
    };
    if parts.len() > 1 {
        format!("{}-{}", base, parts[1])
    } else {
        base
    }
}

/// Parse constraint string like ">= 1.0, < 2.0" into a VersionReq.
/// Normalizes each part to have 3 components.
fn parse_constraint(s: &str) -> Result<VersionReq, String> {
    let parts: Vec<String> = s
        .split(',')
        .map(|p| {
            let p = p.trim();
            // Split on first digit to get operator and version
            if let Some(pos) = p.find(|c: char| c.is_ascii_digit()) {
                let (op, ver) = p.split_at(pos);
                format!("{}{}", op, normalize_version(ver.trim()))
            } else {
                p.to_string()
            }
        })
        .collect();
    let joined = parts.join(", ");
    VersionReq::parse(&joined).map_err(|e| format!("invalid constraint '{s}': {e}"))
}

pub fn register(engine: &ScriptEngine) -> LuaResult<()> {
    let lua = &engine.lua;
    let globals = lua.globals();

    let version_table = lua.create_table()?;

    version_table.set(
        "parse",
        lua.create_function(|lua, s: String| {
            let normalized = normalize_version(&s);
            match Version::parse(&normalized) {
                Ok(v) => {
                    let t = lua.create_table()?;
                    t.set("major", v.major as i64)?;
                    t.set("minor", v.minor as i64)?;
                    t.set("patch", v.patch as i64)?;
                    let pre_str = v.pre.to_string();
                    if pre_str.is_empty() {
                        t.set("pre", LuaValue::Nil)?;
                    } else {
                        t.set("pre", pre_str)?;
                    }
                    Ok(Some(t))
                }
                Err(e) => Err(LuaError::runtime(format!(
                    "invalid version '{s}': {e}"
                ))),
            }
        })?,
    )?;

    version_table.set(
        "compare",
        lua.create_function(|_, (a, b): (String, String)| {
            let va = Version::parse(&normalize_version(&a))
                .map_err(|e| LuaError::runtime(format!("invalid version '{a}': {e}")))?;
            let vb = Version::parse(&normalize_version(&b))
                .map_err(|e| LuaError::runtime(format!("invalid version '{b}': {e}")))?;
            Ok(match va.cmp(&vb) {
                std::cmp::Ordering::Less => -1,
                std::cmp::Ordering::Equal => 0,
                std::cmp::Ordering::Greater => 1,
            })
        })?,
    )?;

    version_table.set(
        "satisfies",
        lua.create_function(|_, (version, constraint): (String, String)| {
            let v = Version::parse(&normalize_version(&version))
                .map_err(|e| LuaError::runtime(format!("invalid version '{version}': {e}")))?;
            let req = parse_constraint(&constraint)
                .map_err(|e| LuaError::runtime(e))?;
            Ok(req.matches(&v))
        })?,
    )?;

    globals.set("Version", version_table)?;
    Ok(())
}
