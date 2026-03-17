use crate::{config::Config, eaccess, game_obj::GameObjRegistry, game_state::GameState, script_engine::ScriptEngine, xml_parser::{ObjHand, ObjCategory, StreamParser}};
use anyhow::Result;
use std::sync::{Arc, Mutex, RwLock};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{broadcast, mpsc};
use tracing::{error, info, warn};

pub async fn run(config: Config, engine: Arc<ScriptEngine>) -> Result<()> {
    let listener = TcpListener::bind(&config.listen).await?;
    info!("Listening on {}", config.listen);
    let active = Arc::new(tokio::sync::Mutex::new(false));
    loop {
        let (client, addr) = listener.accept().await?;
        let mut is_active = active.lock().await;
        if *is_active {
            warn!("Second client attempted to connect from {addr} — rejecting (single-client proxy)");
            drop(client); // close the connection
            continue;
        }
        *is_active = true;
        drop(is_active);
        let cfg = config.clone();
        let eng = engine.clone();
        let active2 = active.clone();
        tokio::spawn(async move {
            if let Err(e) = handle_client(client, cfg, eng).await {
                error!("Session error: {e:#}");
            }
            *active2.lock().await = false;
            info!("Client disconnected, ready for new connection");
        });
    }
}

/// Read one `\n`-terminated line from `r` during the login handshake.
/// Returns the line including the newline character.
async fn read_handshake_line<R>(r: &mut R) -> Result<String>
where
    R: AsyncReadExt + Unpin,
{
    let mut line = Vec::new();
    let mut byte = [0u8; 1];
    loop {
        let n = r.read(&mut byte).await?;
        if n == 0 {
            anyhow::bail!("connection closed during handshake");
        }
        line.push(byte[0]);
        if byte[0] == b'\n' {
            break;
        }
    }
    Ok(String::from_utf8_lossy(&line).into_owned())
}

/// Version string sent to the game server during handshake.
/// Matches Lich5 front-end.rb: Frontend::CLIENT_STRING
const CLIENT_STRING: &str = "/FE:WRAYTH /VERSION:1.0.1.28 /P:WIN_UNKNOWN /XML";

async fn handle_client(client: TcpStream, config: Config, engine: Arc<ScriptEngine>) -> Result<()> {
    let session = if let Some(s) = config.session.clone() {
        s
    } else {
        eaccess::authenticate(
            config.account.as_deref().unwrap_or(""),
            config.password.as_deref().unwrap_or(""),
            &config.game,
            config.character.as_deref().unwrap_or(""),
        ).await?
    };
    info!("Connecting to game server {}:{}", session.host, session.port);

    let server = TcpStream::connect((session.host.as_str(), session.port)).await?;
    let (mut srv_r, srv_w) = server.into_split();
    let (mut cli_r, cli_w) = client.into_split();

    {
        let mut srv_w = srv_w; // rebind as mutable

        // Handshake — matches Lich5 main.rb supports_gsl? branch:
        //
        //   client_string = $_CLIENT_.gets   # read key from client
        //   Game._puts(client_string)         # forward to game server
        //   $_CLIENT_.gets                    # read version string from client (discard)
        //   Frontend.send_handshake(CLIENT_STRING)  # send our version + setup commands
        //
        // Step 1: read key from client (Wrayth sends /K{key} it got from the command line)
        let key_line = read_handshake_line(&mut cli_r).await?;
        srv_w.write_all(key_line.as_bytes()).await?;

        // Step 2: read client's version string and discard it
        let _client_version = read_handshake_line(&mut cli_r).await?;

        // Step 3: send our version string + setup commands (Frontend.send_handshake)
        //   - CLIENT_STRING   → tells server we're Wrayth with XML support
        //   - <c> × 2        → ready signals (with 300ms delay matching Lich5)
        //   - <c>_injury 2   → send detailed injury/scar data
        //   - <c>_flag Display Inventory Boxes 1
        //   - <c>_flag Display Dialog Boxes 0
        srv_w.write_all(format!("{CLIENT_STRING}\n").as_bytes()).await?;
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
        srv_w.write_all(b"<c>\n").await?;
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
        srv_w.write_all(b"<c>\n").await?;
        srv_w.write_all(b"<c>_injury 2\n").await?;
        srv_w.write_all(b"<c>_flag Display Inventory Boxes 1\n").await?;
        srv_w.write_all(b"<c>_flag Display Dialog Boxes 0\n").await?;

        let game_state: Arc<RwLock<GameState>> = {
            let gs = GameState {
                name: config.character.clone().unwrap_or_default(),
                game: crate::game_state::Game::from_code(&config.game),
                ..Default::default()
            };
            Arc::new(RwLock::new(gs))
        };

        // Broadcast channel: downstream raw bytes → waiting scripts (waitfor)
        let (downstream_tx, _) = broadcast::channel::<Arc<Vec<u8>>>(256);

        // Upstream commands channel: hook chain output → sink_drain → server
        let (upstream_tx, mut upstream_rx) = mpsc::unbounded_channel::<String>();

        // Single client-bound write channel: all output (game + respond()) → client_writer → cli_w
        let (client_tx, mut client_rx) = mpsc::unbounded_channel::<Vec<u8>>();

        // Wire engine
        engine.set_game_state(game_state.clone());
        let game_objs: Arc<Mutex<GameObjRegistry>> = Arc::new(Mutex::new(GameObjRegistry::new()));
        engine.set_game_objs(game_objs.clone());
        let go_arc = game_objs.clone();
        // Broadcast channel: upstream raw bytes → listening scripts (upstream_get)
        let (upstream_broadcast, _) = broadcast::channel::<Vec<u8>>(256);
        engine.set_upstream_broadcast(upstream_broadcast.clone());

        engine.set_downstream_channel(downstream_tx.clone());

        // Wire upstream sink: Lua calls sink(line) → upstream_tx
        let up_tx_for_sink = upstream_tx.clone();
        engine.set_upstream_sink(move |line: String| {
            if let Err(e) = up_tx_for_sink.send(line) {
                warn!("upstream_sink send failed: {e}");
            }
        });

        // Wire client_tx into engine for respond()
        let client_tx_respond = client_tx.clone();
        engine.set_respond_sink(move |s| { let _ = client_tx_respond.send(s.into_bytes()); });

        // Open database for this character session
        match crate::db::Db::open(&config.db_path) {
            Ok(db) => {
                engine.set_db(db, config.character.as_deref().unwrap_or(""), &config.game);

                // Construct Infomon with a second Db handle
                match crate::db::Db::open(&config.db_path) {
                    Ok(infomon_db) => {
                        let character = config.character.as_deref().unwrap_or("").to_string();
                        let game = config.game.clone();
                        let infomon = crate::infomon::Infomon::new(infomon_db, &character, &game);
                        *engine.infomon.lock().unwrap() = Some(infomon);

                        // Register Infomon downstream hook
                        let infomon_ref = engine.infomon.clone();
                        engine.downstream_hooks.lock().unwrap().add_sync(
                            "__revenant_infomon",
                            move |line| {
                                if let Some(ref mut im) = *infomon_ref.lock().unwrap() {
                                    for l in line.lines() {
                                        im.parse(l);
                                    }
                                }
                                Some(line.to_string())
                            },
                        );
                    }
                    Err(e) => tracing::warn!("Failed to open Infomon DB: {e}"),
                }
            }
            Err(e) => tracing::warn!("Failed to open DB at {}: {e}", config.db_path),
        }

        // Game-specific data subfolder: GS3/GS4/GST → "gs", DR/DRT/DRF → "dr"
        let data_game_dir = if config.game.starts_with("DR") { "dr" } else { "gs" };

        // Load spell definitions from scripts_dir/data/{game}/
        let spell_path = format!("{}/data/{}/effect-list.xml", config.scripts_dir, data_game_dir);
        match crate::spell_data::SpellList::load(&spell_path) {
            Ok(sl) => {
                tracing::info!("Loaded {} spell definitions from {}", sl.len(), spell_path);
                engine.set_spell_list(std::sync::Arc::new(sl));
            }
            Err(e) => tracing::warn!("Failed to load {spell_path}: {e} (spell system disabled)"),
        }

        // Load gameobj type data from scripts_dir/data/{game}/
        let type_path = format!("{}/data/{}/gameobj-data.xml", config.scripts_dir, data_game_dir);
        match crate::type_data::TypeData::load(&type_path) {
            Ok(td) => {
                tracing::info!("Loaded gameobj type data from {}", type_path);
                engine.set_type_data(std::sync::Arc::new(td));
            }
            Err(e) => tracing::warn!("Failed to load {type_path}: {e} (type data disabled)"),
        }

        engine.install_lua_api()?;

        // Launch autostart.lua if it exists
        let autostart_path = format!("{}/autostart.lua", config.scripts_dir);
        if std::path::Path::new(&autostart_path).exists() {
            engine.start_script("autostart", &autostart_path, vec![])?;
        }

        let ds_tx = downstream_tx.clone();
        let gs = game_state.clone();
        let client_tx_down = client_tx.clone();
        let parser_game = crate::game_state::Game::from_code(&config.game);
        let ds_hooks = engine.downstream_hooks.clone();
        let ds_lua = engine.lua.clone();
        let game_log = engine.game_log.clone();
        let safe_flag = engine.safe_to_respond.clone();

        // Downstream: server → parse XML → hook chain → client_writer
        let mut down_handle = tokio::spawn(async move {
            // Log raw game stream to /tmp/revenant_stream.log for debugging
            let mut stream_log: Option<std::fs::File> = {
                use std::io::Write;
                match std::fs::OpenOptions::new()
                    .create(true).write(true).truncate(true)
                    .open("/tmp/revenant_stream.log")
                {
                    Ok(mut f) => {
                        let _ = writeln!(f, "=== revenant stream log ===");
                        Some(f)
                    }
                    Err(e) => { warn!("Could not open stream log: {e}"); None }
                }
            };

            let mut buf = vec![0u8; 4096];
            let mut parser = StreamParser::new(parser_game);
            loop {
                let n = srv_r.read(&mut buf).await?;
                if n == 0 { break; }
                let raw = buf[..n].to_vec();
                let chunk = String::from_utf8_lossy(&raw).to_string();

                if let Some(ref mut f) = stream_log {
                    use std::io::Write;
                    let _ = f.write_all(&raw);
                }
                {
                    let events = parser.feed(&chunk);

                    // Update safe_to_respond flag for DR output injection safety
                    safe_flag.store(parser.safe_to_respond(), std::sync::atomic::Ordering::Relaxed);

                    // Game log capture (no locks on gs or go needed)
                    for event in &events {
                        if let crate::xml_parser::XmlEvent::Text { ref content } = event {
                            let mut log = game_log.lock().unwrap();
                            if log.len() >= 2000 { log.pop_front(); }
                            log.push_back(content.clone());
                        }
                    }

                    // Game object registry updates (go lock only)
                    {
                        let mut go: std::sync::MutexGuard<'_, GameObjRegistry> = go_arc.lock().unwrap();
                        for event in &events {
                            match event {
                                crate::xml_parser::XmlEvent::GameObjCreate { id, noun, name, category, status } => {
                                    match category {
                                        ObjCategory::Npc  => go.new_npc(id, noun, name, status.as_deref()),
                                        ObjCategory::Loot => go.new_loot(id, noun, name),
                                        ObjCategory::Pc   => go.new_pc(id, noun, name, status.as_deref()),
                                        ObjCategory::RoomDesc => go.new_room_desc(id, noun, name),
                                        ObjCategory::Inv { container } => {
                                            go.new_inv(id, noun, name, container.as_deref(), None, None);
                                        }
                                    }
                                }
                                crate::xml_parser::XmlEvent::GameObjHandUpdate { hand, id, noun, name } => {
                                    match hand {
                                        ObjHand::Right => go.new_right_hand(id, noun, name),
                                        ObjHand::Left  => go.new_left_hand(id, noun, name),
                                    }
                                }
                                crate::xml_parser::XmlEvent::GameObjHandClear { hand } => {
                                    match hand {
                                        ObjHand::Right => go.right_hand = None,
                                        ObjHand::Left  => go.left_hand = None,
                                    }
                                }
                                crate::xml_parser::XmlEvent::ComponentClear { component_id } => {
                                    match component_id.as_str() {
                                        "room objs"    => { go.clear_loot(); go.clear_npcs(); }
                                        "room players" => go.clear_pcs(),
                                        "inv"          => go.clear_inv(),
                                        _ => {}
                                    }
                                }
                                crate::xml_parser::XmlEvent::RoomId { .. }
                                | crate::xml_parser::XmlEvent::RoomCountBump => {
                                    go.clear_for_room_transition();
                                }
                                crate::xml_parser::XmlEvent::FamiliarRoomName { .. } => {
                                    go.clear_familiar();
                                }
                                crate::xml_parser::XmlEvent::FamiliarObjCreate { id, noun, name, category } => {
                                    match category {
                                        ObjCategory::Npc      => go.new_fam_npc(id, noun, name),
                                        ObjCategory::Loot     => go.new_fam_loot(id, noun, name),
                                        ObjCategory::Pc       => go.new_fam_pc(id, noun, name),
                                        ObjCategory::RoomDesc => go.new_fam_room_desc(id, noun, name),
                                        _ => {}
                                    }
                                }
                                _ => {}
                            }
                        }
                    }

                    // GameState updates (gs lock only)
                    {
                        let mut state = gs.write().unwrap_or_else(|e| e.into_inner());
                        for event in events {
                            state.apply(event);
                        }
                    }
                }
                // Always broadcast raw bytes to ds_tx first (for waitfor)
                let _ = ds_tx.send(Arc::new(raw.clone()));

                // Run downstream hook chain (including Lua hooks)
                let result = {
                    let chain = ds_hooks.lock().unwrap();
                    chain.process_with_lua(&ds_lua, &chunk)
                        .unwrap_or_else(|e| {
                            warn!("Lua downstream hook error: {e}");
                            Some(chunk.clone())
                        })
                };
                match result {
                    Some(s) => {
                        if let Err(e) = client_tx_down.send(s.into_bytes()) {
                            warn!("client_tx send failed (downstream): {e}");
                            break;
                        }
                    }
                    None => {
                        // Hook suppressed this chunk — don't send to client
                    }
                }
            }
            anyhow::Ok(())
        });

        let upstream_tx_up = upstream_tx.clone();
        let upstream_broadcast_up = upstream_broadcast.clone();
        let engine_up = engine.clone();

        // Upstream: client → line-buffer → hook chain → upstream_tx
        let mut up_handle = tokio::spawn(async move {
            let mut buf = vec![0u8; 4096];
            let mut line_buf = String::new();
            loop {
                let n = cli_r.read(&mut buf).await?;
                if n == 0 { break; }
                line_buf.push_str(&String::from_utf8_lossy(&buf[..n]));
                // Process complete lines
                while let Some(pos) = line_buf.find('\n') {
                    let line = line_buf[..=pos].to_string();
                    line_buf = line_buf[pos + 1..].to_string();

                    // Dispatch: semicolon commands are consumed; others run through hook chain
                    let dispatch_result = crate::dispatch::dispatch(line.trim_end_matches('\n'), &engine_up).await;
                    let forwarded = match dispatch_result {
                        crate::dispatch::DispatchResult::Forward(s) => Some(s + "\n"),
                        crate::dispatch::DispatchResult::Consumed => None,
                    };

                    if let Some(fwd) = forwarded {
                        *engine_up.last_upstream_time.lock().unwrap() = std::time::Instant::now();

                        // Broadcast upstream line to listening scripts
                        let _ = upstream_broadcast_up.send(fwd.trim_end().as_bytes().to_vec());

                        // Run upstream hook chain (including Lua hooks)
                        let result = {
                            let chain = engine_up.upstream_hooks.lock().unwrap();
                            chain.process_with_lua(&engine_up.lua, &fwd)
                                .unwrap_or_else(|e| {
                                    warn!("Lua upstream hook error: {e}");
                                    Some(fwd.clone())
                                })
                        };
                        match result {
                            Some(s) => {
                                if let Err(e) = upstream_tx_up.send(s) {
                                    warn!("upstream_tx send failed: {e}");
                                    break;
                                }
                            }
                            None => {
                                // Hook suppressed this line — don't forward to server
                            }
                        }
                    }
                }
            }
            anyhow::Ok(())
        });

        // sink_drain: upstream_rx → server write (owns srv_w exclusively)
        let mut sink_handle = tokio::spawn(async move {
            while let Some(line) = upstream_rx.recv().await {
                let mut bytes = line.into_bytes();
                if !bytes.ends_with(b"\n") {
                    bytes.push(b'\n');
                }
                srv_w.write_all(&bytes).await?;
            }
            anyhow::Ok(())
        });

        // client_writer: client_rx → client write (owns cli_w exclusively)
        let mut client_handle = tokio::spawn(async move {
            let mut cli_w = cli_w;
            while let Some(data) = client_rx.recv().await {
                cli_w.write_all(&data).await?;
            }
            anyhow::Ok(())
        });

        let session_result = tokio::select! {
            r = &mut down_handle => r.map_err(|e| anyhow::anyhow!("task: {e}")).and_then(|r| r),
            r = &mut up_handle => r.map_err(|e| anyhow::anyhow!("task: {e}")).and_then(|r| r),
            r = &mut sink_handle => r.map_err(|e| anyhow::anyhow!("task: {e}")).and_then(|r| r),
            r = &mut client_handle => r.map_err(|e| anyhow::anyhow!("task: {e}")).and_then(|r| r),
        };
        // Always abort remaining tasks and clear sinks — even on error
        down_handle.abort();
        up_handle.abort();
        sink_handle.abort();
        client_handle.abort();
        *engine.upstream_sink.lock().unwrap() = None;
        *engine.downstream_tx.lock().unwrap() = None;
        *engine.upstream_broadcast_tx.lock().unwrap() = None;
        engine.want_upstream.lock().unwrap().clear();
        engine.upstream_lines_tx.lock().unwrap().clear();
        engine.upstream_lines_rx.lock().unwrap().clear();
        *engine.respond_sink.lock().unwrap() = None;
        *engine.game_state.lock().unwrap() = None;
        engine.clear_game_objs();
        *engine.infomon.lock().unwrap() = None;
        engine.downstream_hooks.lock().unwrap().remove("__revenant_infomon");
        info!("Session ended, engine state cleared");
        session_result?;
    }

    Ok(())
}
