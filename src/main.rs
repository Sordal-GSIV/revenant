use clap::Parser;
use std::sync::Arc;
use tracing::info;

fn main() -> anyhow::Result<()> {
    // WSL2 display backend fix: force X11 over Wayland to avoid broken pipe errors
    // and arboard/winit clipboard mismatch. X11/XWayland works reliably in WSLg.
    #[cfg(target_os = "linux")]
    if std::env::var_os("WSL_DISTRO_NAME").is_some() {
        // SAFETY: called before any threads are spawned (top of main).
        unsafe { std::env::remove_var("WAYLAND_DISPLAY"); }
    }

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("revenant=info".parse()?),
        )
        .init();

    let config = revenant::config::Config::parse();

    #[cfg(feature = "monitor")]
    if config.monitor || config.account.is_none() {
        return run_with_gui(config);
    }

    // Normal headless mode — requires --account and --character
    let account = config
        .account
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("--account required in headless mode"))?
        .to_string();
    let password = config.password.as_deref().unwrap_or("").to_string();
    let character = config
        .character
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("--character required in headless mode"))?
        .to_string();

    let mut resolved = config.clone();
    resolved.account = Some(account);
    resolved.password = Some(password);
    resolved.character = Some(character);

    tokio::runtime::Runtime::new()?.block_on(async {
        let engine = Arc::new(revenant::script_engine::ScriptEngine::new());
        engine.set_scripts_dir(&resolved.scripts_dir);

        if let Some(ref p) = resolved.map_path {
            if let Err(e) = engine.load_map(p) {
                tracing::warn!("Could not load map from {p}: {e}");
            }
        }

        info!("Revenant starting — listening on {}", resolved.listen);
        revenant::proxy::run(resolved, engine).await
    })
}

fn launch_game_client(config: &revenant::config::Config, session: &revenant::eaccess::Session) {
    let listen_port = config.listen.split(':').last()
        .and_then(|p| p.parse::<u16>().ok())
        .unwrap_or(4900);
    let key = &session.key;

    // Avalon on macOS: write a SAL file and open it with the Avalon app.
    // Matches Lich5 main.rb: launcher_cmd = "open -n -b Avalon \"%1\""
    // and @launch_data.collect! { |line| line.sub(/GAMEPORT=.../).sub(/GAMEHOST=.../) }
    #[cfg(target_os = "macos")]
    if config.frontend == "avalon" {
        let sal_lines: Vec<String> = session.raw_fields.iter().map(|(k, v)| {
            let v = if k.to_uppercase() == "GAMEHOST" { "127.0.0.1".to_string() }
                    else if k.to_uppercase() == "GAMEPORT" { listen_port.to_string() }
                    else { v.clone() };
            format!("{k}={v}")
        }).collect();
        let sal_content = sal_lines.join("\n");
        let sal_path = std::env::temp_dir().join(format!("revenant_{}.sal", listen_port));
        if let Err(e) = std::fs::write(&sal_path, &sal_content) {
            tracing::error!("Failed to write Avalon SAL file: {e}");
            return;
        }
        tracing::info!("Launching Avalon with SAL: {}", sal_path.display());
        match std::process::Command::new("open")
            .args(["-n", "-b", "Avalon", sal_path.to_str().unwrap_or("")])
            .spawn()
        {
            Ok(child) => tracing::info!("Avalon launched (pid {})", child.id()),
            Err(e) => tracing::error!("Failed to launch Avalon: {e}"),
        }
        return;
    }

    let game_code_short = if config.game.starts_with("DR") { "DR" } else { "GS" };

    let cmd = if let Some(ref custom) = config.custom_launch {
        custom.replace("%port%", &listen_port.to_string())
              .replace("%key%", key)
    } else {
        let exe = match config.frontend.as_str() {
            "wizard" => "Wizard.Exe",
            _ => "Wrayth.exe",
        };
        let host = match config.frontend.as_str() {
            "wizard" => "127.0.0.1",
            _ => "localhost",
        };
        let args = match config.frontend.as_str() {
            "wizard" => format!("/G{game_code_short}/H{host} /P{listen_port} /K{key}"),
            _ => format!("/G{game_code_short}/H{host}/P{listen_port}/K{key}"),
        };
        // On Windows or WSL2: run the exe directly via WSL interop / native.
        // On plain Linux/macOS without interop: prefix with wine.
        let is_wsl2 = std::env::var_os("WSL_DISTRO_NAME").is_some();
        if cfg!(target_os = "windows") || is_wsl2 {
            format!("{exe} {args}")
        } else {
            format!("wine {exe} {args}")
        }
    };

    let dir = config.custom_launch_dir.as_deref().unwrap_or(".");

    tracing::info!("Launching game client: {}", cmd.replace(key.as_str(), "[KEY]"));

    let parts: Vec<&str> = cmd.splitn(2, ' ').collect();
    if parts.is_empty() { return; }

    let mut command = std::process::Command::new(parts[0]);
    if parts.len() > 1 {
        command.args(parts[1].split_whitespace());
    }
    if dir != "." {
        command.current_dir(dir);
    }

    match command.spawn() {
        Ok(child) => tracing::info!("Game client launched (pid {})", child.id()),
        Err(e) => tracing::error!("Failed to launch game client: {e}"),
    }
}

#[cfg(feature = "monitor")]
fn run_with_gui(config: revenant::config::Config) -> anyhow::Result<()> {
    use revenant::monitor::MonitorApp;

    // Show the login window first
    let login_result = show_login_window()?;

    let mut resolved = config.clone();
    resolved.account = Some(login_result.account);
    resolved.password = Some(login_result.password);
    resolved.game = login_result.game_code;
    resolved.character = Some(login_result.character);
    resolved.session = login_result.session;
    resolved.frontend = login_result.frontend.as_str().to_string();
    resolved.custom_launch = login_result.custom_launch;
    resolved.custom_launch_dir = login_result.custom_launch_dir;

    let engine = Arc::new(revenant::script_engine::ScriptEngine::new());
    engine.set_scripts_dir(&resolved.scripts_dir);
    if let Some(ref p) = resolved.map_path {
        if let Err(e) = engine.load_map(p) {
            tracing::warn!("Could not load map: {e}");
        }
    }

    // Launch game client if we have a pre-obtained session key
    if let Some(ref session) = resolved.session {
        launch_game_client(&resolved, session);
    }

    // Spawn proxy in background tokio runtime
    let rt = tokio::runtime::Runtime::new()?;
    let engine_clone = engine.clone();
    let config_clone = resolved.clone();
    rt.spawn(async move {
        if let Err(e) = revenant::proxy::run(config_clone, engine_clone).await {
            tracing::error!("Proxy error: {e:#}");
        }
    });

    // Run egui monitor on main thread
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
    )
    .map_err(|e| anyhow::anyhow!("egui: {e}"))?;

    Ok(())
}

#[cfg(feature = "monitor")]
fn show_login_window() -> anyhow::Result<revenant::login::LoginResult> {
    use revenant::login::{LoginApp, LoginResult};
    use std::sync::{Arc, Mutex};

    let result_slot: Arc<Mutex<Option<LoginResult>>> = Arc::new(Mutex::new(None));
    let result_slot2 = result_slot.clone();

    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("Revenant — Login")
            .with_inner_size([480.0, 400.0]),
        ..Default::default()
    };

    eframe::run_native(
        "Revenant Login",
        options,
        Box::new(move |_cc| {
            Ok(Box::new(LoginAppWrapper {
                app: LoginApp::new(),
                result_slot: result_slot2.clone(),
            }))
        }),
    )
    .map_err(|e| anyhow::anyhow!("egui login: {e}"))?;

    let result = result_slot
        .lock()
        .unwrap()
        .take()
        .ok_or_else(|| anyhow::anyhow!("Login cancelled"))?;
    Ok(result)
}

#[cfg(feature = "monitor")]
struct LoginAppWrapper {
    app: revenant::login::LoginApp,
    result_slot: std::sync::Arc<std::sync::Mutex<Option<revenant::login::LoginResult>>>,
}

#[cfg(feature = "monitor")]
impl eframe::App for LoginAppWrapper {
    fn update(&mut self, ctx: &eframe::egui::Context, frame: &mut eframe::Frame) {
        self.app.update(ctx, frame);
        if let Some(ref r) = self.app.result {
            *self.result_slot.lock().unwrap() = Some(r.clone());
        }
    }
}
