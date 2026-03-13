use clap::Parser;
use std::sync::Arc;
use tracing::info;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("revenant=info".parse()?),
        )
        .init();
    let config = revenant::config::Config::parse();
    info!("Revenant starting — listening on {}", config.listen);

    let engine = Arc::new(revenant::script_engine::ScriptEngine::new());
    engine.set_scripts_dir(&config.scripts_dir);

    if let Some(ref p) = config.map_path {
        if let Err(e) = engine.load_map(p) {
            tracing::warn!("Could not load map from {p}: {e}");
        }
    }

    revenant::proxy::run(config, engine).await
}
