use crate::script_engine::ScriptEngine;
use std::sync::Arc;

pub enum DispatchResult {
    Forward(String),
    Consumed,
}

/// Parse args Lich5-style: index 0 = full string, index 1 = first token, etc.
/// Handles double-quoted strings as single tokens (quotes stripped from value).
pub fn parse_args(rest: &str) -> Vec<String> {
    if rest.is_empty() {
        return vec![];
    }
    let mut args = vec![rest.to_string()]; // args[0] = full string (Lich5 compat)
    let mut current = String::new();
    let mut in_quotes = false;
    for ch in rest.chars() {
        match ch {
            '"' => in_quotes = !in_quotes,
            ' ' | '\t' if !in_quotes => {
                if !current.is_empty() {
                    args.push(current.clone());
                    current.clear();
                }
            }
            _ => current.push(ch),
        }
    }
    if !current.is_empty() {
        args.push(current);
    }
    args
}

pub async fn dispatch(raw: &str, engine: &Arc<ScriptEngine>) -> DispatchResult {
    // Strip <c> prefix added by Stormfront/Wrayth
    let line = raw.strip_prefix("<c>").unwrap_or(raw);

    // Not a semicolon command — pass through original raw bytes unchanged
    if !line.starts_with(';') {
        return DispatchResult::Forward(raw.to_string());
    }

    // Strip the leading ';'
    let after_semi = &line[1..];
    let (cmd, rest) = match after_semi.find(|c: char| c.is_whitespace()) {
        Some(pos) => (&after_semi[..pos], after_semi[pos + 1..].trim()),
        None => (after_semi, ""),
    };

    match cmd {
        "k" | "kill" => {
            if rest.is_empty() || rest == "all" {
                engine.kill_all().await;
            } else {
                engine.kill_script(rest).await;
            }
            DispatchResult::Consumed
        }
        "p" | "pause" => {
            if rest.is_empty() || rest == "all" {
                engine.pause_all();
            } else {
                engine.pause_script(rest);
            }
            DispatchResult::Consumed
        }
        "u" | "unpause" => {
            if rest.is_empty() || rest == "all" {
                engine.unpause_all();
            } else {
                engine.unpause_script(rest);
            }
            DispatchResult::Consumed
        }
        "l" | "list" => {
            let names: Vec<String> = {
                let running = engine.running.lock().unwrap();
                running
                    .iter()
                    .filter(|(_, h)| !h.is_finished())
                    .map(|(name, _)| name.clone())
                    .collect()
            };
            if names.is_empty() {
                engine.respond("No scripts running.");
            } else {
                engine.respond(&format!("Running: {}", names.join(", ")));
            }
            DispatchResult::Consumed
        }
        "e" | "exec" => {
            let code = rest.to_string();
            let engine_clone = engine.clone();
            tokio::spawn(async move {
                if let Err(e) = engine_clone.eval_lua(&code).await {
                    engine_clone.respond(&format!("exec error: {e}"));
                }
            });
            DispatchResult::Consumed
        }
        name => {
            let scripts_dir = engine.scripts_dir.lock().unwrap().clone();
            let path = format!("{}/{}.lua", scripts_dir, name);
            if std::path::Path::new(&path).exists() {
                let args = parse_args(rest);
                match engine.start_script(name, &path, args) {
                    Ok(()) => {}
                    Err(e) => engine.respond(&format!("Failed to start script '{name}': {e}")),
                }
            } else {
                engine.respond(&format!("Script not found: {name}"));
            }
            DispatchResult::Consumed
        }
    }
}
