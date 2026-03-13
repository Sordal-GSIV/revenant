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
    loop {
        let (client, addr) = listener.accept().await?;
        info!("Client connected from {addr}");
        let cfg = config.clone();
        let eng = engine.clone();
        tokio::spawn(async move {
            if let Err(e) = handle_client(client, cfg, eng).await {
                error!("Session error: {e:#}");
            }
        });
    }
}

async fn handle_client(client: TcpStream, config: Config, engine: Arc<ScriptEngine>) -> Result<()> {
    let session = eaccess::authenticate(
        &config.account, &config.password, &config.game, &config.character,
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
        // (respond() stub prints to stdout for now; the channel is wired in Task 5)

        engine.install_lua_api()?;

        // Launch autostart.lua if it exists
        let autostart_path = format!("{}/autostart.lua", config.scripts_dir);
        if std::path::Path::new(&autostart_path).exists() {
            engine.start_script("autostart", &autostart_path)?;
        }

        let ds_tx = downstream_tx.clone();
        let gs = game_state.clone();
        let client_tx_down = client_tx.clone();
        let ds_hooks = engine.downstream_hooks.clone();

        // Downstream: server → parse XML → hook chain → client_writer
        let down = tokio::spawn(async move {
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

        // Upstream: client → line-buffer → hook chain → upstream_tx
        let up = tokio::spawn(async move {
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

                    // Run upstream hook chain
                    // TODO: dispatch in Task 2
                    let result = {
                        let chain = us_hooks.lock().unwrap();
                        chain.process_sync(&line)
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
            anyhow::Ok(())
        });

        // sink_drain: upstream_rx → server write (owns srv_w exclusively)
        let sink_drain = tokio::spawn(async move {
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
        let client_writer = tokio::spawn(async move {
            let mut cli_w = cli_w;
            while let Some(data) = client_rx.recv().await {
                cli_w.write_all(&data).await?;
            }
            anyhow::Ok(())
        });

        tokio::select! {
            r = down          => { r??; }
            r = up            => { r??; }
            r = sink_drain    => { r??; }
            r = client_writer => { r??; }
        }
    }

    info!("Session ended");
    Ok(())
}
