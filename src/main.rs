use clap::Parser;
use std::sync::Arc;
use tracing::info;

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("revenant=info".parse()?),
        )
        .init();

    let config = revenant::config::Config::parse();

    #[cfg(feature = "monitor")]
    if config.monitor {
        return run_with_monitor(config);
    }

    // Normal headless mode
    tokio::runtime::Runtime::new()?.block_on(async {
        let engine = Arc::new(revenant::script_engine::ScriptEngine::new());
        engine.set_scripts_dir(&config.scripts_dir);

        if let Some(ref p) = config.map_path {
            if let Err(e) = engine.load_map(p) {
                tracing::warn!("Could not load map from {p}: {e}");
            }
        }

        info!("Revenant starting — listening on {}", config.listen);
        revenant::proxy::run(config, engine).await
    })
}

#[cfg(feature = "monitor")]
fn run_with_monitor(config: revenant::config::Config) -> anyhow::Result<()> {
    use revenant::monitor::MonitorApp;

    let engine = Arc::new(revenant::script_engine::ScriptEngine::new());
    engine.set_scripts_dir(&config.scripts_dir);
    if let Some(ref p) = config.map_path {
        if let Err(e) = engine.load_map(p) {
            tracing::warn!("Could not load map: {e}");
        }
    }

    // Spawn proxy in background tokio runtime
    let rt = tokio::runtime::Runtime::new()?;
    let engine_clone = engine.clone();
    let config_clone = config.clone();
    rt.spawn(async move {
        if let Err(e) = revenant::proxy::run(config_clone, engine_clone).await {
            tracing::error!("Proxy error: {e:#}");
        }
    });

    // Run egui on main thread
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("Revenant Monitor")
            .with_inner_size([700.0, 500.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Revenant Monitor",
        options,
        Box::new(move |_cc| Ok(Box::new(MonitorApp::new(engine)))),
    ).map_err(|e| anyhow::anyhow!("egui: {e}"))?;

    Ok(())
}
