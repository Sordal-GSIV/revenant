use crate::{config::Config, eaccess, game_state::GameState, xml_parser::parse_chunk};
use anyhow::Result;
use std::sync::{Arc, RwLock};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::broadcast;
use tracing::{error, info};

pub async fn run(config: Config) -> Result<()> {
    let listener = TcpListener::bind(&config.listen).await?;
    info!("Listening on {}", config.listen);
    loop {
        let (client, addr) = listener.accept().await?;
        info!("Client connected from {addr}");
        let cfg = config.clone();
        tokio::spawn(async move {
            if let Err(e) = handle_client(client, cfg).await {
                error!("Session error: {e:#}");
            }
        });
    }
}

async fn handle_client(client: TcpStream, config: Config) -> Result<()> {
    let session = eaccess::authenticate(
        &config.account, &config.password, &config.game, &config.character,
    ).await?;
    info!("Connecting to game server {}:{}", session.host, session.port);

    let server = TcpStream::connect((session.host.as_str(), session.port)).await?;
    let (mut srv_r, mut srv_w) = server.into_split();
    let (mut cli_r, mut cli_w) = client.into_split();

    // Send session key to game server
    srv_w.write_all(session.key.as_bytes()).await?;
    srv_w.write_all(b"\n").await?;

    let game_state: Arc<RwLock<GameState>> = Arc::new(RwLock::new(GameState::default()));
    // Broadcast channel: downstream raw bytes → waiting scripts
    let (downstream_tx, _) = broadcast::channel::<Arc<Vec<u8>>>(256);
    let ds_tx = downstream_tx.clone();

    // Downstream: server → client (parse XML, update GameState, run hooks)
    let gs = game_state.clone();
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
            // Broadcast to waiting scripts (ignore if no subscribers)
            let _ = ds_tx.send(Arc::new(raw));
            // TODO: run downstream hook chain (Task 9)
            cli_w.write_all(&buf[..n]).await?;
        }
        anyhow::Ok(())
    });

    // Upstream: client → server (run hooks, forward)
    let up = tokio::spawn(async move {
        let mut buf = vec![0u8; 4096];
        loop {
            let n = cli_r.read(&mut buf).await?;
            if n == 0 { break; }
            // TODO: run upstream hook chain (Task 9)
            srv_w.write_all(&buf[..n]).await?;
        }
        anyhow::Ok(())
    });

    tokio::select! { r = down => { r??; } r = up => { r??; } }
    info!("Session ended");
    Ok(())
}
