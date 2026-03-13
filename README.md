# Revenant

A Rust + Lua scripting proxy for [GemStone IV](https://www.play.net/gs4/) (Simutronics). Spiritual successor to [Lich5](https://github.com/lich-developer/lich5) — replaces the Ruby runtime with a fast Rust engine and hot-reloadable Lua scripts.

> **Status:** v0.1 — core engine complete. Connects Wrayth to GemStone IV, parses the XML stream, and executes Lua scripts with full hook chain support.

## What it does

- Authenticates with the SGE eAccess server (SSL, XOR password hashing)
- Proxies the TCP connection between your frontend (Wrayth) and GemStone IV
- Parses the GemStone XML stream into typed events → live `GameState`
- Runs Lua scripts as tokio coroutines with `pause`, `waitfor`, `put`, `fput`
- Upstream and downstream hook chains — intercept and modify any line
- Per-character SQLite settings (`CharSettings`, `UserVars`)

## Architecture

```
Wrayth ──TCP──▶ Revenant ──TCP──▶ GemStone IV
                   │
                   ├─ XML parser → GameState
                   ├─ Downstream hook chain (Lua + Rust)
                   ├─ Upstream hook chain (Lua + Rust)
                   └─ Script engine (mlua 0.10, Lua 5.4)
```

## Building

Requires Rust stable (1.75+). Lua is vendored — no system Lua needed.

```bash
cargo build --release
```

## Running

```bash
# Not yet wired (main.rs is a stub) — use as a library for now
# See tests/integration.rs for usage examples
cargo test
```

## Lua API

Scripts run in the `revenant-scripts` repo. Available globals:

| Global | Description |
|--------|-------------|
| `put(cmd)` | Send a command to the game server |
| `fput(cmd)` | Send a command (prompt-sync TODO) |
| `pause(secs)` | Async sleep |
| `waitfor(pattern [, timeout])` | Block until pattern appears in stream |
| `respond(text)` | Echo text to client (stdout in v0.1) |
| `GameState.health` | Current HP (and all other vital fields) |
| `GameState.roundtime()` | Seconds of roundtime remaining |
| `DownstreamHook.add(name, fn)` | Register a downstream hook |
| `UpstreamHook.add(name, fn)` | Register an upstream hook |
| `CharSettings["key"]` | Per-character SQLite settings |
| `UserVars["key"]` | Game-wide SQLite vars |
| `Script.kill(name)` | Kill a running script |
| `Script.list()` | List running scripts |

## Example script

```lua
-- Drink a potion when HP drops below 50%
DownstreamHook.add("auto_heal", function(line)
    if GameState.health < GameState.max_health * 0.5 then
        put("drink my potion")
    end
    return line
end)
```

## Project layout

```
src/
  main.rs           — entry point (stub)
  lib.rs            — module exports
  config.rs         — Config struct
  eaccess.rs        — SGE eAccess authentication
  xml_parser.rs     — XmlEvent enum + parse_chunk
  game_state.rs     — GameState + apply(XmlEvent)
  proxy.rs          — TCP proxy (listener + bidirectional forwarding)
  hook_chain.rs     — HookChain (sync + Lua registry entries)
  script_engine.rs  — ScriptEngine (mlua VM + coroutine runner)
  db.rs             — SQLite (char_settings, user_vars, map)
  lua_api/
    mod.rs          — register_all()
    primitives.rs   — put/fput/pause/waitfor/respond
    game_state.rs   — GameState Lua bindings
    hooks.rs        — UpstreamHook/DownstreamHook
    script.rs       — Script.kill/list/args
    settings.rs     — CharSettings/UserVars
tests/
  eaccess.rs        — password hash unit test
  game_state.rs     — GameState struct + apply() tests
  xml_parser.rs     — XML parsing tests
  hook_chain.rs     — HookChain sync tests
  script_engine.rs  — Lua API tests
  db.rs             — SQLite roundtrip tests
  integration.rs    — v1 acceptance test (healing hook)
```

## Tech stack

| Crate | Version | Purpose |
|-------|---------|---------|
| tokio | 1 | Async runtime |
| mlua | 0.10 | Lua 5.4 (vendored) |
| quick-xml | 0.39 | XML stream parser |
| rusqlite | 0.32 | SQLite (bundled) |
| tokio-rustls | 0.26 | TLS for eAccess |
| anyhow / thiserror | 1/2 | Error handling |
| clap | 4 | CLI args |
| tracing | 0.1 | Logging |

## Related

- [revenant-scripts](https://github.com/Sordal-GSIV/revenant-scripts) — default and community Lua scripts
- [Lich5](https://github.com/lich-developer/lich5) — the Ruby proxy this replaces
- [GemStone IV](https://www.play.net/gs4/) — the game
