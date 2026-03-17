use std::path::PathBuf;

pub struct AppConfig {
    pub theme: String,
    pub window_width: f32,
    pub window_height: f32,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            theme: "slate".to_string(),
            window_width: 480.0,
            window_height: 400.0,
        }
    }
}

impl AppConfig {
    fn config_dir() -> PathBuf {
        let mut p = dirs::config_dir().unwrap_or_else(|| PathBuf::from("."));
        p.push("revenant");
        p
    }

    fn config_path() -> PathBuf {
        Self::config_dir().join("config.toml")
    }

    fn legacy_path() -> PathBuf {
        Self::config_dir().join("theme.toml")
    }

    pub fn load() -> Self {
        let config_path = Self::config_path();
        let legacy_path = Self::legacy_path();

        // Migration: if config.toml doesn't exist but theme.toml does, migrate
        if !config_path.exists() && legacy_path.exists() {
            let mut migrated = Self::default();
            if let Ok(contents) = std::fs::read_to_string(&legacy_path) {
                for line in contents.lines() {
                    let line = line.trim();
                    if let Some(value) = line.strip_prefix("theme") {
                        let value = value.trim().strip_prefix('=').unwrap_or("").trim();
                        let value = value.trim_matches('"').trim_matches('\'');
                        match value {
                            "slate" | "ember" | "fantasy" => {
                                migrated.theme = value.to_string();
                            }
                            _ => {}
                        }
                    }
                }
            }
            // Write config.toml and delete theme.toml
            migrated.save();
            let _ = std::fs::remove_file(&legacy_path);
            return migrated;
        }

        match std::fs::read_to_string(&config_path) {
            Ok(contents) => {
                let mut cfg = Self::default();
                for line in contents.lines() {
                    let line = line.trim();
                    if let Some(value) = line.strip_prefix("theme") {
                        let value = value.trim().strip_prefix('=').unwrap_or("").trim();
                        let value = value.trim_matches('"').trim_matches('\'');
                        match value {
                            "slate" | "ember" | "fantasy"
                            | "slate_light" | "ember_light" | "fantasy_light" => {
                                cfg.theme = value.to_string();
                            }
                            _ => {}
                        }
                    } else if let Some(value) = line.strip_prefix("window_width") {
                        let value = value.trim().strip_prefix('=').unwrap_or("").trim();
                        if let Ok(v) = value.parse::<f32>() {
                            cfg.window_width = v;
                        }
                    } else if let Some(value) = line.strip_prefix("window_height") {
                        let value = value.trim().strip_prefix('=').unwrap_or("").trim();
                        if let Ok(v) = value.parse::<f32>() {
                            cfg.window_height = v;
                        }
                    }
                }
                cfg
            }
            Err(_) => Self::default(),
        }
    }

    pub fn save(&self) {
        let dir = Self::config_dir();
        let _ = std::fs::create_dir_all(&dir);
        let contents = format!(
            "theme = \"{}\"\nwindow_width = {}\nwindow_height = {}\n",
            self.theme, self.window_width, self.window_height
        );
        let _ = std::fs::write(Self::config_path(), contents);
    }

    pub fn to_theme(&self) -> egui_theme::Theme {
        match self.theme.as_str() {
            "ember" => egui_theme::Theme::ember(),
            "fantasy" => egui_theme::Theme::fantasy(),
            "slate_light" => egui_theme::Theme::slate_light(),
            "ember_light" => egui_theme::Theme::ember_light(),
            "fantasy_light" => egui_theme::Theme::fantasy_light(),
            _ => egui_theme::Theme::slate(),
        }
    }
}
