use crate::{config::Config, eaccess, frontend::Capability, game_obj::GameObjRegistry, game_state::GameState, gsl_converter::GslConverter, script_engine::ScriptEngine, xml_parser::{ObjHand, ObjCategory, StreamParser}};
use anyhow::Result;
use std::sync::{Arc, Mutex, RwLock};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{broadcast, mpsc};
use tracing::{error, info, warn};

pub async fn run(config: Config, engine: Arc<ScriptEngine>) -> Result<()> {
    if config.without_frontend {
        return run_headless(config, engine).await;
    }

    let listener = TcpListener::bind(&config.listen).await?;
    info!("Listening on {}", config.listen);
    let active = Arc::new(tokio::sync::Mutex::new(false));
    loop {
        let (client, addr) = listener.accept().await?;
        let mut is_active = active.lock().await;
        if *is_active {
            warn!("Second client attempted to connect from {addr} — rejecting (single-client proxy)");
            drop(client);
            continue;
        }
        *is_active = true;
        drop(is_active);
        let cfg = config.clone();
        let eng = engine.clone();
        let active2 = active.clone();
        tokio::spawn(async move {
            if cfg.reconnect {
                loop {
                    // For reconnect with a frontend, we only retry the server session,
                    // not the client accept. The client is already connected.
                    // But the client socket is consumed, so reconnect with frontend
                    // is not supported in the same way — just run once.
                    if let Err(e) = handle_session(Some(client), cfg.clone(), eng.clone()).await {
                        error!("Session error: {e:#}");
                    }
                    // With a frontend client, we can't reconnect (socket consumed)
                    break;
                }
            } else {
                if let Err(e) = handle_session(Some(client), cfg, eng).await {
                    error!("Session error: {e:#}");
                }
            }
            *active2.lock().await = false;
            info!("Client disconnected, ready for new connection");
        });
    }
}

/// Headless mode: no client TcpListener. Connect directly to game server,
/// logging downstream output via tracing instead of a client socket.
async fn run_headless(config: Config, engine: Arc<ScriptEngine>) -> Result<()> {
    info!("Starting in headless mode (no frontend)");
    if config.reconnect {
        loop {
            match handle_session(None, config.clone(), engine.clone()).await {
                Ok(()) => {
                    info!("Session ended cleanly");
                    break;
                }
                Err(e) => {
                    warn!("Connection lost: {e:#}");
                    info!("Reconnecting in {}s...", config.reconnect_delay);
                    tokio::time::sleep(std::time::Duration::from_secs(config.reconnect_delay)).await;
                }
            }
        }
    } else {
        handle_session(None, config, engine).await?;
    }
    Ok(())
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

/// Build an XML state snapshot from the current GameState for detachable client sync.
fn build_state_snapshot(gs: &GameState) -> String {
    let mut xml = String::new();

    // Vitals
    xml.push_str(&format!(
        "<progressBar id=\"health\" value=\"{}\" text=\"health {}/{}\"/>\n",
        gs.health, gs.health, gs.max_health
    ));
    xml.push_str(&format!(
        "<progressBar id=\"mana\" value=\"{}\" text=\"mana {}/{}\"/>\n",
        gs.mana, gs.mana, gs.max_mana
    ));
    xml.push_str(&format!(
        "<progressBar id=\"spirit\" value=\"{}\" text=\"spirit {}/{}\"/>\n",
        gs.spirit, gs.spirit, gs.max_spirit
    ));
    xml.push_str(&format!(
        "<progressBar id=\"stamina\" value=\"{}\" text=\"stamina {}/{}\"/>\n",
        gs.stamina, gs.stamina, gs.max_stamina
    ));
    xml.push_str(&format!(
        "<progressBar id=\"concentration\" value=\"{}\" text=\"concentration {}/{}\"/>\n",
        gs.concentration, gs.concentration, gs.max_concentration
    ));

    // Indicators
    let indicators = [
        ("IconBLEEDING", gs.bleeding), ("IconSTUNNED", gs.stunned),
        ("IconDEAD", gs.dead), ("IconSLEEPING", gs.sleeping),
        ("IconPRONE", gs.prone), ("IconSITTING", gs.sitting),
        ("IconKNEELING", gs.kneeling), ("IconSTANDING", gs.standing),
        ("IconPOISONED", gs.poisoned), ("IconDISEASED", gs.diseased),
        ("IconHIDDEN", gs.hidden), ("IconINVISIBLE", gs.invisible),
        ("IconWEBBED", gs.webbed), ("IconJOINED", gs.joined),
    ];
    for (name, visible) in indicators {
        let v = if visible { "y" } else { "n" };
        xml.push_str(&format!("<indicator id=\"{name}\" visible=\"{v}\"/>\n"));
    }

    // Room info
    if !gs.room_name.is_empty() {
        xml.push_str(&format!(
            "<streamWindow id='room' title='Room' subtitle=\" - [{}]\"/>\n",
            gs.room_name
        ));
    }

    xml
}

/// Write a session file for the detachable client connection.
fn write_session_file(scripts_dir: &str, character: &str, host: &str, port: u16, game: &str) {
    let sessions_dir = format!("{scripts_dir}/_data/sessions");
    if let Err(e) = std::fs::create_dir_all(&sessions_dir) {
        warn!("Failed to create sessions dir {sessions_dir}: {e}");
        return;
    }
    let path = format!("{sessions_dir}/{character}.session");
    let json = format!(
        "{{\"name\": \"{character}\", \"host\": \"{host}\", \"port\": {port}, \"game\": \"{game}\"}}"
    );
    if let Err(e) = std::fs::write(&path, &json) {
        warn!("Failed to write session file {path}: {e}");
    } else {
        info!("Session file written: {path}");
    }
}

/// Delete the session file for the detachable client.
fn delete_session_file(scripts_dir: &str, character: &str) {
    let path = format!("{scripts_dir}/_data/sessions/{character}.session");
    if std::fs::remove_file(&path).is_ok() {
        info!("Session file removed: {path}");
    }
}

/// Core session handler. If `client` is None, runs in headless mode.
async fn handle_session(client: Option<TcpStream>, config: Config, engine: Arc<ScriptEngine>) -> Result<()> {
    let headless = client.is_none();

    // Authenticate
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
    let (srv_r, mut srv_w) = server.into_split();

    // Handshake with server
    if let Some(c) = client {
        let (mut cli_r, cli_w) = c.into_split();

        // Step 1: read key from client (Wrayth sends /K{key})
        let key_line = read_handshake_line(&mut cli_r).await?;
        srv_w.write_all(key_line.as_bytes()).await?;

        // Step 2: read client's version string and discard it
        let _client_version = read_handshake_line(&mut cli_r).await?;

        // Step 3: send our version string + setup commands
        srv_w.write_all(format!("{CLIENT_STRING}\n").as_bytes()).await?;
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
        srv_w.write_all(b"<c>\n").await?;
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
        srv_w.write_all(b"<c>\n").await?;
        srv_w.write_all(b"<c>_injury 2\n").await?;
        srv_w.write_all(b"<c>_flag Display Inventory Boxes 1\n").await?;
        srv_w.write_all(b"<c>_flag Display Dialog Boxes 0\n").await?;

        run_proxy_loop(Some((cli_r, cli_w)), srv_r, srv_w, config, engine, headless).await
    } else {
        // Headless: send login key + version directly
        srv_w.write_all(format!("{}\n", session.key).as_bytes()).await?;
        srv_w.write_all(format!("{CLIENT_STRING}\n").as_bytes()).await?;
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
        srv_w.write_all(b"<c>\n").await?;
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
        srv_w.write_all(b"<c>\n").await?;
        srv_w.write_all(b"<c>_injury 2\n").await?;
        srv_w.write_all(b"<c>_flag Display Inventory Boxes 1\n").await?;
        srv_w.write_all(b"<c>_flag Display Dialog Boxes 0\n").await?;

        // Set frontend to Unknown in headless mode
        *engine.frontend.lock().unwrap() = crate::frontend::Frontend::Unknown;

        run_proxy_loop(None, srv_r, srv_w, config, engine, headless).await
    }
}

/// The main proxy loop — shared between headless and client modes.
async fn run_proxy_loop(
    client_halves: Option<(tokio::net::tcp::OwnedReadHalf, tokio::net::tcp::OwnedWriteHalf)>,
    mut srv_r: tokio::net::tcp::OwnedReadHalf,
    srv_w: tokio::net::tcp::OwnedWriteHalf,
    config: Config,
    engine: Arc<ScriptEngine>,
    headless: bool,
) -> Result<()> {
    let game_state: Arc<RwLock<GameState>> = {
        let gs = GameState {
            name: config.character.clone().unwrap_or_default(),
            game: crate::game_state::Game::from_code(&config.game),
            ..Default::default()
        };
        Arc::new(RwLock::new(gs))
    };

    // Broadcast channel: downstream raw bytes -> waiting scripts (waitfor)
    let (downstream_tx, _) = broadcast::channel::<Arc<Vec<u8>>>(256);

    // Upstream commands channel: hook chain output -> sink_drain -> server
    let (upstream_tx, mut upstream_rx) = mpsc::unbounded_channel::<String>();

    // Single client-bound write channel: all output (game + respond()) -> client_writer -> cli_w
    let (client_tx, mut client_rx) = mpsc::unbounded_channel::<Vec<u8>>();

    // Wire engine
    engine.set_game_state(game_state.clone());
    let game_objs: Arc<Mutex<GameObjRegistry>> = Arc::new(Mutex::new(GameObjRegistry::new()));
    engine.set_game_objs(game_objs.clone());
    let go_arc = game_objs.clone();
    // Broadcast channel: upstream raw bytes -> listening scripts (upstream_get)
    let (upstream_broadcast, _) = broadcast::channel::<Vec<u8>>(256);
    engine.set_upstream_broadcast(upstream_broadcast.clone());

    engine.set_downstream_channel(downstream_tx.clone());

    // Wire upstream sink: Lua calls sink(line) -> upstream_tx
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
            if let Err(e) = db.vacuum() {
                tracing::warn!("DB VACUUM failed (non-fatal): {e}");
            }
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

    // Game-specific data subfolder: GS3/GS4/GST -> "gs", DR/DRT/DRF -> "dr"
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

    // Set frontend from config (unless headless already set it)
    if !headless {
        *engine.frontend.lock().unwrap() = crate::frontend::Frontend::from_name(&config.frontend);
    }

    engine.install_lua_api()?;

    // Launch autostart.lua if it exists
    let autostart_path = format!("{}/autostart.lua", config.scripts_dir);
    if std::path::Path::new(&autostart_path).exists() {
        engine.start_script("autostart", &autostart_path, vec![])?;
    }

    // Spawn detachable client listener if configured
    let detachable_handle = if let Some(det_port) = config.detachable_client_port {
        Some(spawn_detachable_listener(
            config.detachable_client_host.clone(),
            det_port,
            game_state.clone(),
            downstream_tx.clone(),
            upstream_tx.clone(),
            upstream_broadcast.clone(),
            engine.clone(),
            config.scripts_dir.clone(),
            config.character.clone().unwrap_or_default(),
            config.game.clone(),
            engine.frontend.clone(),
        ))
    } else {
        None
    };

    let ds_tx = downstream_tx.clone();
    let gs = game_state.clone();
    let client_tx_down = client_tx.clone();
    let parser_game = crate::game_state::Game::from_code(&config.game);
    let ds_hooks = engine.downstream_hooks.clone();
    let ds_lua = engine.lua.clone();
    let game_log = engine.game_log.clone();
    let safe_flag = engine.safe_to_respond.clone();
    let engine_fe = engine.frontend.clone();
    let is_headless = headless;

    // Downstream: server -> parse XML -> hook chain -> client_writer
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
        let mut gsl_converter = GslConverter::new();
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
                    if is_headless {
                        // In headless mode, log downstream to tracing (info level)
                        // The text is still available via detachable client if configured
                        for line in s.lines() {
                            // Only log non-empty, non-XML lines for readability
                            let trimmed = line.trim();
                            if !trimmed.is_empty() && !trimmed.starts_with('<') {
                                info!("[game] {trimmed}");
                            }
                        }
                    } else {
                        let frontend = *engine_fe.lock().unwrap();
                        let output = if frontend.supports(Capability::Gsl) {
                            match gsl_converter.convert(&s) {
                                Some(converted) => converted,
                                None => continue, // suppressed by GSL converter
                            }
                        } else if frontend == crate::frontend::Frontend::Genie
                            || frontend == crate::frontend::Frontend::Frostbite
                        {
                            strip_room_number(&s)
                        } else {
                            s
                        };
                        if let Err(e) = client_tx_down.send(output.into_bytes()) {
                            warn!("client_tx send failed (downstream): {e}");
                            break;
                        }
                    }
                }
                None => {
                    // Hook suppressed this chunk — don't send to client
                }
            }
        }
        anyhow::Ok(())
    });

    // Upstream and client writer tasks (only when we have a client)
    let mut up_handle: Option<tokio::task::JoinHandle<Result<()>>> = None;
    #[allow(unused_assignments)]
    let mut client_handle: Option<tokio::task::JoinHandle<Result<()>>> = None;

    if let Some((cli_r, cli_w)) = client_halves {
        let mut cli_r = cli_r;
        let upstream_tx_up = upstream_tx.clone();
        let upstream_broadcast_up = upstream_broadcast.clone();
        let engine_up = engine.clone();

        // Upstream: client -> line-buffer -> hook chain -> upstream_tx
        up_handle = Some(tokio::spawn(async move {
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
        }));

        // client_writer: client_rx -> client write (owns cli_w exclusively)
        let mut cli_w = cli_w;
        client_handle = Some(tokio::spawn(async move {
            while let Some(data) = client_rx.recv().await {
                cli_w.write_all(&data).await?;
            }
            anyhow::Ok(())
        }));
    } else {
        // Headless: drain client_rx (for respond() output that goes to detachable clients only)
        // We still need to consume from client_rx to avoid blocking senders
        client_handle = Some(tokio::spawn(async move {
            while let Some(_data) = client_rx.recv().await {
                // In headless mode, respond() output is logged in the downstream task.
                // client_rx is consumed so senders don't block. Detachable clients
                // get data from the broadcast channel, not from client_rx.
            }
            anyhow::Ok(())
        }));
    }

    // sink_drain: upstream_rx -> server write (owns srv_w exclusively)
    let mut sink_handle = tokio::spawn(async move {
        let mut srv_w = srv_w;
        while let Some(line) = upstream_rx.recv().await {
            let mut bytes = line.into_bytes();
            if !bytes.ends_with(b"\n") {
                bytes.push(b'\n');
            }
            srv_w.write_all(&bytes).await?;
        }
        anyhow::Ok(())
    });

    // Wait for any task to finish
    let session_result = match (up_handle, client_handle) {
        (Some(mut uh), Some(mut ch)) => {
            let r = tokio::select! {
                r = &mut down_handle => r.map_err(|e| anyhow::anyhow!("task: {e}")).and_then(|r| r),
                r = &mut uh => r.map_err(|e| anyhow::anyhow!("task: {e}")).and_then(|r| r),
                r = &mut sink_handle => r.map_err(|e| anyhow::anyhow!("task: {e}")).and_then(|r| r),
                r = &mut ch => r.map_err(|e| anyhow::anyhow!("task: {e}")).and_then(|r| r),
            };
            uh.abort();
            ch.abort();
            r
        }
        (None, Some(mut ch)) => {
            let r = tokio::select! {
                r = &mut down_handle => r.map_err(|e| anyhow::anyhow!("task: {e}")).and_then(|r| r),
                r = &mut sink_handle => r.map_err(|e| anyhow::anyhow!("task: {e}")).and_then(|r| r),
                r = &mut ch => r.map_err(|e| anyhow::anyhow!("task: {e}")).and_then(|r| r),
            };
            ch.abort();
            r
        }
        _ => {
            tokio::select! {
                r = &mut down_handle => r.map_err(|e| anyhow::anyhow!("task: {e}")).and_then(|r| r),
                r = &mut sink_handle => r.map_err(|e| anyhow::anyhow!("task: {e}")).and_then(|r| r),
            }
        }
    };

    // Always abort remaining tasks and clear sinks — even on error
    down_handle.abort();
    sink_handle.abort();
    if let Some(dh) = detachable_handle { dh.abort(); }
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

    Ok(())
}

/// Spawn a detachable client listener task. Returns a JoinHandle.
/// When a detachable client connects, it receives a state snapshot then forks
/// into the existing downstream broadcast and upstream channels.
fn spawn_detachable_listener(
    host: String,
    port: u16,
    game_state: Arc<RwLock<GameState>>,
    downstream_tx: broadcast::Sender<Arc<Vec<u8>>>,
    upstream_tx: mpsc::UnboundedSender<String>,
    upstream_broadcast: broadcast::Sender<Vec<u8>>,
    engine: Arc<ScriptEngine>,
    scripts_dir: String,
    character: String,
    game: String,
    engine_fe: Arc<Mutex<crate::frontend::Frontend>>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let addr = format!("{host}:{port}");
        let listener = match TcpListener::bind(&addr).await {
            Ok(l) => { info!("Detachable client listener on {addr}"); l }
            Err(e) => { error!("Failed to bind detachable client listener on {addr}: {e}"); return; }
        };

        loop {
            let (client, peer) = match listener.accept().await {
                Ok(c) => c,
                Err(e) => { warn!("Detachable accept error: {e}"); continue; }
            };
            info!("Detachable client connected from {peer}");

            // Write session file
            write_session_file(&scripts_dir, &character, &host, port, &game);

            let (mut det_r, mut det_w) = client.into_split();

            // Send state snapshot
            let snapshot = {
                let gs = game_state.read().unwrap_or_else(|e| e.into_inner());
                build_state_snapshot(&gs)
            };
            if let Err(e) = det_w.write_all(snapshot.as_bytes()).await {
                warn!("Failed to send snapshot to detachable client: {e}");
                delete_session_file(&scripts_dir, &character);
                continue;
            }

            // Fork downstream: subscribe to broadcast and forward to detachable client
            let mut ds_rx = downstream_tx.subscribe();
            let ds_fe = engine_fe.clone();
            let ds_hooks = engine.downstream_hooks.clone();
            let ds_lua = engine.lua.clone();
            let mut det_down = tokio::spawn(async move {
                let mut det_gsl = GslConverter::new();
                loop {
                    match ds_rx.recv().await {
                        Ok(raw) => {
                            let chunk = String::from_utf8_lossy(&raw).to_string();
                            // Run hook chain for this client too
                            let result = {
                                let chain = ds_hooks.lock().unwrap();
                                chain.process_with_lua(&ds_lua, &chunk)
                                    .unwrap_or_else(|e| {
                                        warn!("Lua downstream hook error (detachable): {e}");
                                        Some(chunk.clone())
                                    })
                            };
                            if let Some(s) = result {
                                let frontend = *ds_fe.lock().unwrap();
                                let output = if frontend.supports(Capability::Gsl) {
                                    match det_gsl.convert(&s) {
                                        Some(converted) => converted,
                                        None => continue,
                                    }
                                } else if frontend == crate::frontend::Frontend::Genie
                                    || frontend == crate::frontend::Frontend::Frostbite
                                {
                                    strip_room_number(&s)
                                } else {
                                    s
                                };
                                if det_w.write_all(output.as_bytes()).await.is_err() {
                                    break;
                                }
                            }
                        }
                        Err(broadcast::error::RecvError::Lagged(n)) => {
                            warn!("Detachable client lagged by {n} messages");
                        }
                        Err(broadcast::error::RecvError::Closed) => break,
                    }
                }
            });

            // Upstream from detachable client
            let up_tx = upstream_tx.clone();
            let up_bcast = upstream_broadcast.clone();
            let eng = engine.clone();
            let mut det_up = tokio::spawn(async move {
                let mut buf = vec![0u8; 4096];
                let mut line_buf = String::new();
                loop {
                    let n = match det_r.read(&mut buf).await {
                        Ok(0) | Err(_) => break,
                        Ok(n) => n,
                    };
                    line_buf.push_str(&String::from_utf8_lossy(&buf[..n]));
                    while let Some(pos) = line_buf.find('\n') {
                        let line = line_buf[..=pos].to_string();
                        line_buf = line_buf[pos + 1..].to_string();

                        let dispatch_result = crate::dispatch::dispatch(line.trim_end_matches('\n'), &eng).await;
                        let forwarded = match dispatch_result {
                            crate::dispatch::DispatchResult::Forward(s) => Some(s + "\n"),
                            crate::dispatch::DispatchResult::Consumed => None,
                        };

                        if let Some(fwd) = forwarded {
                            *eng.last_upstream_time.lock().unwrap() = std::time::Instant::now();
                            let _ = up_bcast.send(fwd.trim_end().as_bytes().to_vec());
                            let result = {
                                let chain = eng.upstream_hooks.lock().unwrap();
                                chain.process_with_lua(&eng.lua, &fwd)
                                    .unwrap_or_else(|e| {
                                        warn!("Lua upstream hook error (detachable): {e}");
                                        Some(fwd.clone())
                                    })
                            };
                            if let Some(s) = result {
                                if up_tx.send(s).is_err() {
                                    break;
                                }
                            }
                        }
                    }
                }
            });

            // Wait for either direction to finish
            tokio::select! {
                _ = &mut det_down => {}
                _ = &mut det_up => {}
            }
            det_down.abort();
            det_up.abort();

            // Delete session file on disconnect
            delete_session_file(&scripts_dir, &character);
            info!("Detachable client disconnected from {peer}");
        }
    })
}

/// Strip room numbers from streamWindow subtitle attributes for frontends that
/// don't handle them well (Genie, Frostbite).
///
/// Matches patterns like `] (12345)"` or `] (**)"` inside subtitle attributes
/// of `<streamWindow` tags with `id='room'` or `id='main'`.
fn strip_room_number(s: &str) -> String {
    use regex::Regex;
    use std::sync::OnceLock;
    static RE: OnceLock<Regex> = OnceLock::new();
    // Only strip if the line contains a streamWindow tag for room or main
    if !s.contains("<streamWindow") || !(s.contains("id='room'") || s.contains("id='main'")) {
        return s.to_string();
    }
    let re = RE.get_or_init(|| {
        Regex::new(r#"\] \((?:\d+|\*\*)\)"#).unwrap()
    });
    re.replace_all(s, "]").to_string()
}
