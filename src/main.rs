#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

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

    #[cfg(feature = "login-gui")]
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

        if resolved.without_frontend {
            info!("Revenant starting in headless mode (no frontend)");
        } else {
            info!("Revenant starting — listening on {}", resolved.listen);
        }
        revenant::proxy::run(resolved, engine).await
    })
}

/// Read a Simutronics install directory from the Windows registry.
///
/// Matches Lich5 init.rb: reads HKLM\SOFTWARE\WOW6432Node\Simutronics\{STORM32|WIZ32}\Directory
///
/// - Native Windows: uses the winreg crate.
/// - WSL2: invokes reg.exe via Windows interop (/mnt/c/Windows/System32/reg.exe).
/// - Linux + Wine: invokes `wine reg query`.
/// - macOS: returns None (Avalon uses a SAL file, no registry needed).
#[allow(dead_code)]
fn simu_registry_dir(subkey: &str) -> Option<String> {
    #[cfg(target_os = "windows")]
    {
        use winreg::enums::*;
        use winreg::RegKey;
        let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
        let path = format!("SOFTWARE\\WOW6432Node\\Simutronics\\{subkey}");
        let key = hklm.open_subkey(&path).ok()?;
        key.get_value::<String, _>("Directory").ok()
    }

    #[cfg(not(target_os = "windows"))]
    {
        let reg_path = format!("HKLM\\SOFTWARE\\WOW6432Node\\Simutronics\\{subkey}");
        let is_wsl2 = std::env::var_os("WSL_DISTRO_NAME").is_some();
        let (prog, args): (&str, Vec<String>) = if is_wsl2 {
            ("/mnt/c/Windows/System32/reg.exe",
             vec!["query".into(), reg_path.clone(), "/v".into(), "Directory".into()])
        } else {
            ("wine", vec!["reg".into(), "query".into(), reg_path.clone(),
                          "/v".into(), "Directory".into()])
        };
        let out = std::process::Command::new(prog).args(&args).output().ok()?;
        let text = String::from_utf8_lossy(&out.stdout);
        // reg.exe output:  "    Directory    REG_SZ    C:\path\to\dir"
        for line in text.lines() {
            let parts: Vec<&str> = line.splitn(4, "REG_SZ").collect();
            if parts.len() == 2 {
                let dir = parts[1].trim().to_string();
                if !dir.is_empty() {
                    return Some(dir);
                }
            }
        }
        None
    }
}

#[allow(dead_code)]
fn launch_game_client(config: &revenant::config::Config, session: &revenant::eaccess::Session) {
    let listen_port = config.listen.split(':').next_back()
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

    // Resolve game client directory: explicit config → registry → current dir.
    let reg_subkey = match config.frontend.as_str() {
        "wizard" => "WIZ32",
        _ => "STORM32",
    };
    let registry_dir = simu_registry_dir(reg_subkey);
    let raw_dir: String = config.custom_launch_dir
        .as_deref()
        .map(|s| s.to_string())
        .or(registry_dir)
        .unwrap_or_else(|| ".".to_string());

    // On WSL2 the registry returns a Windows path (C:\...).
    // Convert to the Linux mount path (/mnt/c/...) so Rust can use it.
    let is_wsl2 = std::env::var_os("WSL_DISTRO_NAME").is_some();
    let dir: String = if is_wsl2 && raw_dir.len() >= 2 && raw_dir.as_bytes()[1] == b':' {
        let drive = raw_dir[..1].to_lowercase();
        let rest = raw_dir[2..].replace('\\', "/");
        format!("/mnt/{drive}{rest}")
    } else {
        raw_dir.replace('\\', "/")
    };
    let dir = dir.as_str();

    // Custom launch command: substitute %port% and %key%, then split into exe + args.
    if let Some(ref custom) = config.custom_launch {
        let expanded = custom.replace("%port%", &listen_port.to_string())
                             .replace("%key%", key);
        tracing::info!("Launching (custom): {}", expanded.replace(key.as_str(), "[KEY]"));
        let parts: Vec<&str> = expanded.splitn(2, ' ').collect();
        if parts.is_empty() { return; }
        let mut command = std::process::Command::new(parts[0]);
        if parts.len() > 1 { command.args(parts[1].split_whitespace()); }
        if dir != "." { command.current_dir(dir); }
        match command.spawn() {
            Ok(child) => tracing::info!("Game client launched (pid {})", child.id()),
            Err(e) => tracing::error!("Failed to launch game client: {e}"),
        }
        return;
    }

    // Standard launch: build exe path from launch dir + exe name so the exe is
    // found even when it's not on PATH (typical Windows install scenario).
    let exe_name = match config.frontend.as_str() {
        "wizard" => "Wizard.Exe",
        _ => "Wrayth.exe",
    };
    let exe_path = if dir != "." {
        std::path::Path::new(dir).join(exe_name)
    } else {
        std::path::PathBuf::from(exe_name)
    };

    let host = match config.frontend.as_str() {
        "wizard" => "127.0.0.1",
        _ => "localhost",
    };
    let args: Vec<String> = match config.frontend.as_str() {
        "wizard" => vec![format!("/G{game_code_short}/H{host}"),
                         format!("/P{listen_port}"),
                         format!("/K{key}")],
        _ => vec![format!("/G{game_code_short}/H{host}/P{listen_port}/K{key}")],
    };

    tracing::info!("Launching: {} {}", exe_path.display(),
        args.iter().map(|a| a.replace(key.as_str(), "[KEY]")).collect::<Vec<_>>().join(" "));

    // On Windows or WSL2 (interop), run the exe directly.
    // On plain Linux/macOS, prefix with wine.
    let mut command = if cfg!(target_os = "windows") || is_wsl2 {
        std::process::Command::new(&exe_path)
    } else {
        let mut c = std::process::Command::new("wine");
        c.arg(&exe_path);
        c
    };
    command.args(&args);
    // Wrayth/Wizard look for skin files relative to their install directory.
    // Lich5 does Dir.chdir(custom_launch_dir) before spawn — match that.
    if dir != "." {
        command.current_dir(dir);
    }

    match command.spawn() {
        Ok(child) => tracing::info!("Game client launched (pid {})", child.id()),
        Err(e) => tracing::error!("Failed to launch game client: {e}"),
    }
}

#[cfg(feature = "login-gui")]
fn run_with_gui(config: revenant::config::Config) -> anyhow::Result<()> {
    // Show the login window first
    let login_result = show_login_window()?;

    let _theme_name = login_result.theme.clone();
    #[cfg(feature = "monitor")]
    let theme_name = _theme_name.clone();

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

    // If monitor feature is enabled and --monitor flag is set, show the monitor window.
    // Otherwise, just block on the proxy runtime (headless after login).
    #[cfg(feature = "monitor")]
    if resolved.monitor {
        use revenant::monitor::MonitorApp;
        let options = eframe::NativeOptions {
            viewport: eframe::egui::ViewportBuilder::default()
                .with_title("Revenant Monitor")
                .with_inner_size([700.0, 500.0]),
            ..Default::default()
        };
        eframe::run_native(
            "Revenant Monitor",
            options,
            Box::new(move |_cc| Ok(Box::new(MonitorApp::new(engine, &theme_name)))),
        )
        .map_err(|e| anyhow::anyhow!("egui: {e}"))?;
        // Monitor window closed — exit cleanly (don't leave orphan proxy)
        info!("Monitor closed, shutting down");
        std::process::exit(0);
    }

    // No monitor window — block until proxy finishes
    info!("Revenant running (no monitor window)");
    rt.block_on(async {
        tokio::signal::ctrl_c().await.ok();
        info!("Shutting down");
    });
    Ok(())
}

#[cfg(feature = "login-gui")]
fn show_login_window() -> anyhow::Result<revenant::login::LoginResult> {
    use revenant::login::{LoginApp, LoginResult};
    use std::sync::{Arc, Mutex};

    let result_slot: Arc<Mutex<Option<LoginResult>>> = Arc::new(Mutex::new(None));
    let result_slot2 = result_slot.clone();

    let app_config = revenant::app_config::AppConfig::load();
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("Revenant — Login")
            .with_inner_size([app_config.window_width, app_config.window_height])
            .with_min_inner_size([535.0, 535.0]),
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

#[cfg(feature = "login-gui")]
struct LoginAppWrapper {
    app: revenant::login::LoginApp,
    result_slot: std::sync::Arc<std::sync::Mutex<Option<revenant::login::LoginResult>>>,
}

#[cfg(feature = "login-gui")]
impl eframe::App for LoginAppWrapper {
    fn update(&mut self, ctx: &eframe::egui::Context, frame: &mut eframe::Frame) {
        self.app.update(ctx, frame);
        if let Some(ref r) = self.app.result {
            *self.result_slot.lock().unwrap() = Some(r.clone());
        }
    }
}
