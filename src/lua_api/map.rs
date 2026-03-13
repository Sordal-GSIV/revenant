use crate::script_engine::ScriptEngine;
use mlua::prelude::*;

pub fn register(engine: &ScriptEngine) -> LuaResult<()> {
    let lua = &engine.lua;
    let t = lua.create_table()?;

    // Map.find_room(id_or_name) → room table or nil
    let map_data = engine.map_data.clone();
    t.set("find_room", lua.create_function(move |lua, query: LuaValue| {
        let guard = map_data.read().unwrap_or_else(|e| e.into_inner());
        let data = match guard.as_ref() {
            Some(d) => d,
            None => return Ok(LuaValue::Nil),
        };
        let room = match query {
            LuaValue::Integer(id) => data.find_room_by_id(id as u32),
            LuaValue::Number(id) => data.find_room_by_id(id as u32),
            LuaValue::String(s) => {
                let text = s.to_str()?;
                if let Ok(id) = text.parse::<u32>() {
                    data.find_room_by_id(id)
                } else {
                    data.find_room_by_name(&text)
                }
            }
            _ => return Ok(LuaValue::Nil),
        };
        match room {
            None => Ok(LuaValue::Nil),
            Some(r) => {
                let t = lua.create_table()?;
                t.set("id", r.id)?;
                t.set("title", r.title.as_str())?;
                t.set("description", r.description.as_str())?;
                let tags = lua.create_table()?;
                for (i, tag) in r.tags.iter().enumerate() { tags.set(i + 1, tag.as_str())?; }
                t.set("tags", tags)?;
                Ok(LuaValue::Table(t))
            }
        }
    })?)?;

    // Map.find_path(from_id, to_id) → array of command strings, or nil
    let map_data = engine.map_data.clone();
    t.set("find_path", lua.create_function(move |lua, (from, to): (u32, u32)| {
        let guard = map_data.read().unwrap_or_else(|e| e.into_inner());
        let data = match guard.as_ref() {
            Some(d) => d,
            None => return Ok(LuaValue::Nil),
        };
        match data.find_path(from, to) {
            None => Ok(LuaValue::Nil),
            Some(cmds) => {
                let t = lua.create_table()?;
                for (i, cmd) in cmds.iter().enumerate() { t.set(i + 1, cmd.as_str())?; }
                Ok(LuaValue::Table(t))
            }
        }
    })?)?;

    // Map.current_room() → room id from GameState, or nil
    let game_state = engine.game_state.clone();
    t.set("current_room", lua.create_function(move |_, ()| {
        let guard = game_state.lock().unwrap();
        let id = guard.as_ref()
            .and_then(|gs| gs.read().ok())
            .and_then(|gs| gs.room_id);
        match id {
            Some(id) => Ok(LuaValue::Integer(id as i64)),
            None => Ok(LuaValue::Nil),
        }
    })?)?;

    // Map.go2(dest) → true/false
    let map_data = engine.map_data.clone();
    let game_state = engine.game_state.clone();
    let sink = engine.upstream_sink.clone();
    let dtx = engine.downstream_tx.clone();
    t.set("go2", lua.create_async_function(move |_, dest: LuaValue| {
        let map_data = map_data.clone();
        let game_state = game_state.clone();
        let sink = sink.clone();
        let dtx = dtx.clone();
        async move {
            // Resolve destination ID
            let dest_id: u32 = {
                let guard = map_data.read().unwrap_or_else(|e| e.into_inner());
                let data = match guard.as_ref() {
                    Some(d) => d,
                    None => return Err(LuaError::RuntimeError("Map not loaded".into())),
                };
                match &dest {
                    LuaValue::Integer(id) => {
                        if data.find_room_by_id(*id as u32).is_none() {
                            return Ok(false);
                        }
                        *id as u32
                    }
                    LuaValue::Number(id) => *id as u32,
                    LuaValue::String(s) => {
                        let text = s.to_str()?;
                        if let Ok(id) = text.parse::<u32>() { id }
                        else {
                            match data.find_room_by_name(&text)
                                .or_else(|| data.find_room_by_tag(&text)) {
                                Some(r) => r.id,
                                None => return Ok(false),
                            }
                        }
                    }
                    _ => return Ok(false),
                }
            };

            // Get current room ID
            let from_id = {
                let guard = game_state.lock().unwrap();
                guard.as_ref()
                    .and_then(|gs| gs.read().ok())
                    .and_then(|gs| gs.room_id)
            };
            let from_id = match from_id {
                Some(id) => id,
                None => return Err(LuaError::RuntimeError("Current room unknown — wait for room ID from server".into())),
            };

            // Find path
            let path = {
                let guard = map_data.read().unwrap_or_else(|e| e.into_inner());
                guard.as_ref().and_then(|d| d.find_path(from_id, dest_id))
            };
            let path = match path {
                Some(p) => p,
                None => return Ok(false),
            };

            // Execute each command and wait for prompt
            for cmd in path {
                let line = format!("{cmd}\n");
                // Subscribe before send (avoids prompt-miss race — same pattern as fput)
                let mut rx_opt = dtx.lock().unwrap().as_ref().map(|tx| tx.subscribe());
                // Send — drop the sink lock before any .await
                {
                    let guard = sink.lock().unwrap();
                    if let Some(f) = guard.as_ref() { f(line); }
                }
                if let Some(ref mut rx) = rx_opt {
                    let deadline = tokio::time::Instant::now()
                        + tokio::time::Duration::from_secs(10);
                    loop {
                        match tokio::time::timeout_at(deadline, rx.recv()).await {
                            Err(_) => break,
                            Ok(Err(tokio::sync::broadcast::error::RecvError::Lagged(_))) => continue,
                            Ok(Err(_)) => break,
                            Ok(Ok(bytes)) => {
                                if String::from_utf8_lossy(&bytes).contains("<prompt") {
                                    break;
                                }
                            }
                        }
                    }
                }
            }
            Ok(true)
        }
    })?)?;

    // Map.load(path) — reload map from a different JSON file at runtime
    let map_data = engine.map_data.clone();
    t.set("load", lua.create_function(move |_, path: String| {
        match crate::map::MapData::from_file(&path) {
            Ok(data) => {
                *map_data.write().unwrap_or_else(|e| e.into_inner()) = Some(data);
                Ok(true)
            }
            Err(e) => {
                tracing::warn!("Map.load({path}) failed: {e}");
                Ok(false)
            }
        }
    })?)?;

    lua.globals().set("Map", t)?;
    Ok(())
}
