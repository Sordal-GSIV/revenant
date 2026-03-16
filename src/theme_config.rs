use std::path::PathBuf;

pub struct ThemeConfig {
    pub theme: String,
}

impl Default for ThemeConfig {
    fn default() -> Self {
        Self { theme: "slate".to_string() }
    }
}

impl ThemeConfig {
    fn config_dir() -> PathBuf {
        let mut p = dirs::config_dir().unwrap_or_else(|| PathBuf::from("."));
        p.push("revenant");
        p
    }

    fn config_path() -> PathBuf {
        Self::config_dir().join("theme.toml")
    }

    pub fn load() -> Self {
        let path = Self::config_path();
        match std::fs::read_to_string(&path) {
            Ok(contents) => {
                for line in contents.lines() {
                    let line = line.trim();
                    if let Some(value) = line.strip_prefix("theme") {
                        let value = value.trim().strip_prefix('=').unwrap_or("").trim();
                        let value = value.trim_matches('"').trim_matches('\'');
                        match value {
                            "slate" | "ember" | "fantasy" => {
                                return Self { theme: value.to_string() };
                            }
                            _ => {}
                        }
                    }
                }
                Self::default()
            }
            Err(_) => Self::default(),
        }
    }

    pub fn save(&self) {
        let dir = Self::config_dir();
        let _ = std::fs::create_dir_all(&dir);
        let contents = format!("theme = \"{}\"\n", self.theme);
        let _ = std::fs::write(Self::config_path(), contents);
    }

    pub fn to_theme(&self) -> egui_theme::Theme {
        match self.theme.as_str() {
            "ember" => egui_theme::Theme::ember(),
            "fantasy" => egui_theme::Theme::fantasy(),
            _ => egui_theme::Theme::slate(),
        }
    }
}
