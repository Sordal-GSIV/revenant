use mlua::prelude::*;

use crate::script_engine::ScriptEngine;

pub fn register(engine: &ScriptEngine) -> LuaResult<()> {
    let lua = &engine.lua;
    let globals = lua.globals();

    let http_table = lua.create_table()?;

    // Http.get(url) -> { status, body, headers } or nil, error
    http_table.set(
        "get",
        lua.create_async_function(|lua, url: String| async move {
            let client = reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .map_err(|e| LuaError::runtime(format!("http client error: {e}")))?;

            match client.get(&url).send().await {
                Ok(resp) => {
                    let status = resp.status().as_u16();
                    let headers_map = resp.headers().clone();
                    let body = resp
                        .text()
                        .await
                        .unwrap_or_default();

                    let t = lua.create_table()?;
                    t.set("status", status)?;
                    t.set("body", body)?;

                    let headers_table = lua.create_table()?;
                    for (name, value) in &headers_map {
                        if let Ok(v) = value.to_str() {
                            headers_table.set(name.as_str(), v)?;
                        }
                    }
                    t.set("headers", headers_table)?;

                    Ok((Some(t), None::<String>))
                }
                Err(e) => Ok((None, Some(format!("{e}")))),
            }
        })?,
    )?;

    // Http.get_json(url) -> table or nil, error
    http_table.set(
        "get_json",
        lua.create_async_function(|lua, url: String| async move {
            let client = reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .map_err(|e| LuaError::runtime(format!("http client error: {e}")))?;

            match client.get(&url).send().await {
                Ok(resp) => {
                    let status = resp.status().as_u16();
                    if !(200..300).contains(&status) {
                        return Ok((None, Some(format!("HTTP {status}"))));
                    }
                    let body = resp
                        .text()
                        .await
                        .map_err(|e| LuaError::runtime(format!("read body: {e}")))?;

                    match serde_json::from_str::<serde_json::Value>(&body) {
                        Ok(val) => {
                            let lua_val =
                                crate::lua_api::json::json_value_to_lua(&lua, &val)?;
                            Ok((Some(lua_val), None::<String>))
                        }
                        Err(e) => Ok((None, Some(format!("json parse error: {e}")))),
                    }
                }
                Err(e) => Ok((None, Some(format!("{e}")))),
            }
        })?,
    )?;

    globals.set("Http", http_table)?;
    Ok(())
}
