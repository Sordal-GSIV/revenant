use crate::{config::Config, eaccess, game_state::GameState, script_engine::ScriptEngine, xml_parser::parse_chunk};
use anyhow::Result;
use std::sync::{Arc, RwLock};
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

async fn handle_client(client: TcpStream, config: Config, engine: Arc<ScriptEngine>) -> Result<()> {
    let session = eaccess::authenticate(
        config.account.as_deref().unwrap_or(""),
        config.password.as_deref().unwrap_or(""),
        &config.game,
        config.character.as_deref().unwrap_or(""),
    ).await?;
    info!("Connecting to game server {}:{}", session.host, session.port);

    let server = TcpStream::connect((session.host.as_str(), session.port)).await?;
    let (mut srv_r, srv_w) = server.into_split();
    let (mut cli_r, cli_w) = client.into_split();

    // Send session key to game server
    {
        // We need a temporary write to srv_w for the handshake before handing it
        // to sink_drain. Use a one-shot mpsc to deliver the key write first.
        // Simpler: reconstruct via into_split after sending; but WriteHalf doesn't
        // implement reunite without the ReadHalf. Instead, send the key via the
        // upstream channel as the very first message, tagged before the task starts.
        // Actually: just send directly here before spawning sink_drain.
        // We own srv_w exclusively here, so write the key then move it to the task.
        let mut srv_w = srv_w; // rebind as mutable
        srv_w.write_all(session.key.as_bytes()).await?;
        srv_w.write_all(b"\n").await?;

        let game_state: Arc<RwLock<GameState>> = Arc::new(RwLock::new(GameState::default()));

        // Broadcast channel: downstream raw bytes → waiting scripts (waitfor)
        let (downstream_tx, _) = broadcast::channel::<Arc<Vec<u8>>>(256);

        // Upstream commands channel: hook chain output → sink_drain → server
        let (upstream_tx, mut upstream_rx) = mpsc::unbounded_channel::<String>();

        // Single client-bound write channel: all output (game + respond()) → client_writer → cli_w
        let (client_tx, mut client_rx) = mpsc::unbounded_channel::<Vec<u8>>();

        // Wire engine
        engine.set_game_state(game_state.clone());
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
            Ok(db) => engine.set_db(db, config.character.as_deref().unwrap_or(""), &config.game),
            Err(e) => tracing::warn!("Failed to open DB at {}: {e}", config.db_path),
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
        let ds_hooks = engine.downstream_hooks.clone();

        // Downstream: server → parse XML → hook chain → client_writer
        let mut down_handle = tokio::spawn(async move {
            let mut buf = vec![0u8; 4096];
            loop {
                let n = srv_r.read(&mut buf).await?;
                if n == 0 { break; }
                let raw = buf[..n].to_vec();
                let chunk = String::from_utf8_lossy(&raw).to_string();
                {
                    let mut state = gs.write().unwrap_or_else(|e| e.into_inner());
                    for event in parse_chunk(&chunk) {
                        state.apply(event);
                    }
                }
                // Always broadcast raw bytes to ds_tx first (for waitfor)
                let _ = ds_tx.send(Arc::new(raw.clone()));

                // Run downstream hook chain
                // TODO: process_with_lua for Lua downstream hooks
                let result = {
                    let chain = ds_hooks.lock().unwrap();
                    chain.process_sync(&chunk)
                };
                match result {
                    Some(s) => {
                        // Hook chain passed the chunk — send to client
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

        let us_hooks = engine.upstream_hooks.clone();
        let upstream_tx_up = upstream_tx.clone();
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
                        // Run upstream hook chain on forwarded line
                        let result = {
                            let chain = us_hooks.lock().unwrap();
                            chain.process_sync(&fwd)
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
        *engine.respond_sink.lock().unwrap() = None;
        *engine.game_state.lock().unwrap() = None;
        info!("Session ended, engine state cleared");
        session_result?;
    }

    Ok(())
}
