#![cfg(feature = "login-gui")]

use crate::credentials::CredentialStore;
use crate::eaccess::{list_characters, CharacterEntry};
use eframe::egui;
use std::sync::mpsc::{sync_channel, Receiver, SyncSender};

/// Known game codes and their display names.
const GAME_CODES: &[(&str, &str)] = &[
    ("GS3", "GemStone IV"),
    ("DR", "DragonRealms"),
    ("GSF", "GemStone IV Prime F2P"),
];

#[derive(Debug, Clone, PartialEq)]
pub enum Frontend {
    Wrayth,
    Wizard,
    Avalon,
}

impl Frontend {
    pub fn as_str(&self) -> &'static str {
        match self {
            Frontend::Wrayth => "stormfront",
            Frontend::Wizard => "wizard",
            Frontend::Avalon => "avalon",
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            Frontend::Wrayth => "Wrayth",
            Frontend::Wizard => "Wizard",
            Frontend::Avalon => "Avalon",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "wizard" => Frontend::Wizard,
            "avalon" => Frontend::Avalon,
            _ => Frontend::Wrayth,
        }
    }
}

/// The result returned when the user clicks Play.
#[derive(Debug, Clone)]
pub struct LoginResult {
    pub account: String,
    pub password: String,
    pub game_code: String,
    pub character: String,
    pub frontend: Frontend,
    pub custom_launch: Option<String>,
    pub custom_launch_dir: Option<String>,
    pub session: Option<crate::eaccess::Session>,
    pub theme: String,
}

#[derive(Debug, Clone, PartialEq)]
enum MainTab {
    Saved,
    Manual,
    Accounts,
}

#[derive(Debug, Clone, PartialEq)]
enum ConnectState {
    Idle,
    Fetching,
    Connected(Vec<CharacterEntry>),
}

#[derive(Debug, Clone, PartialEq)]
enum AcctSubTab {
    Accounts,
    AddCharacter,
    AddAccount,
    Encryption,
}

#[derive(Debug, Clone, PartialEq)]
enum PlayState {
    Idle,
    Authenticating,
}

#[derive(Clone)]
struct PendingPlay {
    account: String,
    password: String,
    game_code: String,
    character: String,
    frontend: Frontend,
    custom_launch: Option<String>,
    custom_launch_dir: Option<String>,
}

/// Platform-specific launch command suggestions.
fn launch_cmd_suggestions() -> Vec<String> {
    if cfg!(target_os = "windows") {
        vec![
            r"C:\Program Files\Wrayth\Wrayth.exe".into(),
            r"C:\Program Files\Wizard\Wizard.exe".into(),
        ]
    } else if cfg!(target_os = "macos") {
        vec![
            "/Applications/Wrayth.app/Contents/MacOS/Wrayth".into(),
            "/Applications/Wizard.app/Contents/MacOS/Wizard".into(),
        ]
    } else {
        vec![
            "/usr/bin/wrayth".into(),
            "/usr/local/bin/wizard".into(),
        ]
    }
}

/// Platform-specific launch directory suggestions.
fn launch_dir_suggestions() -> Vec<String> {
    if cfg!(target_os = "windows") {
        vec![
            String::new(),
            r"C:\Games".into(),
        ]
    } else {
        vec![
            String::new(),
            dirs::home_dir()
                .map(|h| h.join("games").to_string_lossy().into_owned())
                .unwrap_or_default(),
        ]
    }
}

pub struct LoginApp {
    // Tab selection
    tab: MainTab,
    tab_idx: usize,

    // ── Saved Entry tab ───────────────────────────────────────────────
    store: CredentialStore,
    key: [u8; 32],
    saved_side_tab: usize,
    saved_status: String,

    // ── Manual Entry tab ──────────────────────────────────────────────
    manual_account: String,
    manual_password: String,
    connect_state: ConnectState,
    manual_selected_char: Option<usize>,
    manual_tree_selected: Option<usize>,
    manual_save: bool,
    manual_favorite: bool,
    manual_status: String,
    manual_game_idx: usize,
    manual_game_code: String,
    manual_frontend: Frontend,
    manual_custom_launch_enabled: bool,
    manual_custom_launch: String,
    manual_custom_launch_dir: String,
    fetch_tx: SyncSender<Result<Vec<CharacterEntry>, String>>,
    fetch_rx: Receiver<Result<Vec<CharacterEntry>, String>>,

    // ── Play auth ─────────────────────────────────────────────────────
    play_state: PlayState,
    play_tx: SyncSender<Result<crate::eaccess::Session, String>>,
    play_rx: Receiver<Result<crate::eaccess::Session, String>>,
    pending_play: Option<PendingPlay>,

    // ── Account Management tab ────────────────────────────────────────
    acct_sub_tab: AcctSubTab,
    acct_sub_tab_idx: usize,
    // Accounts sub-tab
    accounts_status: String,
    change_pw_account: Option<String>,
    change_pw_value: String,
    acct_tree_selected: Option<usize>,
    acct_tree_sort_col: Option<usize>,
    acct_tree_sort_asc: bool,
    // Add Character sub-tab
    add_char_account_idx: usize,
    add_char_name: String,
    add_char_game_idx: usize,
    add_char_status: String,
    add_char_frontend: Frontend,
    add_char_custom_launch_enabled: bool,
    add_char_custom_launch: String,
    add_char_custom_launch_dir: String,
    // Add Account sub-tab
    add_acct_username: String,
    add_acct_password: String,
    add_acct_show_password: bool,
    add_acct_status: String,
    add_acct_fetching: bool,
    add_acct_chars: Vec<CharacterEntry>,
    add_acct_tree_selected: Option<usize>,
    add_acct_tx: SyncSender<Result<Vec<CharacterEntry>, String>>,
    add_acct_rx: Receiver<Result<Vec<CharacterEntry>, String>>,

    // ── Theme ─────────────────────────────────────────────────────────
    theme_config: crate::theme_config::ThemeConfig,
    theme_applied: bool,

    // ── Result ────────────────────────────────────────────────────────
    pub result: Option<LoginResult>,
}

impl LoginApp {
    pub fn new() -> Self {
        let (fetch_tx, fetch_rx) = sync_channel(1);
        let (add_acct_tx, add_acct_rx) = sync_channel(1);
        let (play_tx, play_rx) = sync_channel(1);
        let key = CredentialStore::load_or_create_key().unwrap_or([0u8; 32]);
        let store = CredentialStore::load().unwrap_or_default();

        Self {
            tab: MainTab::Saved,
            tab_idx: 0,
            store,
            key,
            saved_side_tab: 0,
            saved_status: String::new(),
            manual_account: String::new(),
            manual_password: String::new(),
            connect_state: ConnectState::Idle,
            manual_selected_char: None,
            manual_tree_selected: None,
            manual_save: false,
            manual_favorite: false,
            manual_status: String::new(),
            manual_game_idx: 0,
            manual_game_code: GAME_CODES[0].0.to_string(),
            manual_frontend: Frontend::Wrayth,
            manual_custom_launch_enabled: false,
            manual_custom_launch: String::new(),
            manual_custom_launch_dir: String::new(),
            fetch_tx,
            fetch_rx,
            play_state: PlayState::Idle,
            play_tx,
            play_rx,
            pending_play: None,
            acct_sub_tab: AcctSubTab::Accounts,
            acct_sub_tab_idx: 0,
            accounts_status: String::new(),
            change_pw_account: None,
            change_pw_value: String::new(),
            acct_tree_selected: None,
            acct_tree_sort_col: None,
            acct_tree_sort_asc: true,
            add_char_account_idx: 0,
            add_char_name: String::new(),
            add_char_game_idx: 0,
            add_char_status: String::new(),
            add_char_frontend: Frontend::Wrayth,
            add_char_custom_launch_enabled: false,
            add_char_custom_launch: String::new(),
            add_char_custom_launch_dir: String::new(),
            add_acct_username: String::new(),
            add_acct_password: String::new(),
            add_acct_show_password: false,
            add_acct_status: String::new(),
            add_acct_fetching: false,
            add_acct_chars: Vec::new(),
            add_acct_tree_selected: None,
            add_acct_tx,
            add_acct_rx,
            theme_config: crate::theme_config::ThemeConfig::load(),
            theme_applied: false,
            result: None,
        }
    }

    fn game_name(code: &str) -> &'static str {
        for &(c, n) in GAME_CODES {
            if c == code {
                return n;
            }
        }
        "Unknown"
    }
}

impl Default for LoginApp {
    fn default() -> Self {
        Self::new()
    }
}

impl LoginApp {
    fn start_play(&mut self, pending: PendingPlay) {
        self.play_state = PlayState::Authenticating;
        self.pending_play = Some(pending.clone());
        let tx = self.play_tx.clone();
        let account = pending.account.clone();
        let password = pending.password.clone();
        let game_code = pending.game_code.clone();
        let character = pending.character.clone();
        std::thread::spawn(move || {
            let rt = match tokio::runtime::Runtime::new() {
                Ok(rt) => rt,
                Err(e) => { let _ = tx.send(Err(e.to_string())); return; }
            };
            let result = rt.block_on(crate::eaccess::authenticate(
                &account, &password, &game_code, &character
            ));
            let _ = tx.send(result.map_err(|e| e.to_string()));
        });
    }
}

impl eframe::App for LoginApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if !self.theme_applied {
            self.theme_config.to_theme().apply(ctx);
            self.theme_applied = true;
        }

        // Poll play auth result
        if let Ok(res) = self.play_rx.try_recv() {
            self.play_state = PlayState::Idle;
            match res {
                Ok(session) => {
                    if let Some(pending) = self.pending_play.take() {
                        self.result = Some(LoginResult {
                            account: pending.account,
                            password: pending.password,
                            game_code: pending.game_code,
                            character: pending.character,
                            frontend: pending.frontend,
                            custom_launch: pending.custom_launch,
                            custom_launch_dir: pending.custom_launch_dir,
                            session: Some(session),
                            theme: self.theme_config.theme.clone(),
                        });
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                }
                Err(e) => {
                    self.manual_status = format!("Auth failed: {e}");
                    self.saved_status = format!("Auth failed: {e}");
                }
            }
        }

        // Poll manual-tab fetch channel
        if let Ok(res) = self.fetch_rx.try_recv() {
            match res {
                Ok(chars) => {
                    self.manual_selected_char = None;
                    self.manual_tree_selected = None;
                    self.connect_state = ConnectState::Connected(chars);
                    self.manual_status.clear();
                }
                Err(e) => {
                    self.connect_state = ConnectState::Idle;
                    self.manual_status = format!("Error: {e}");
                }
            }
        }

        // Poll add-account fetch channel
        if let Ok(res) = self.add_acct_rx.try_recv() {
            self.add_acct_fetching = false;
            match res {
                Ok(chars) => {
                    self.add_acct_chars = chars;
                    self.add_acct_tree_selected = None;
                    self.add_acct_status = format!(
                        "Found {} character(s). Select and click Add Account.",
                        self.add_acct_chars.len()
                    );
                }
                Err(e) => {
                    self.add_acct_status = format!("Error: {e}");
                }
            }
        }

        // Auto-switch to Manual tab if no accounts
        if self.tab == MainTab::Saved && self.store.accounts.is_empty() {
            self.tab = MainTab::Manual;
            self.tab_idx = 1;
        }

        // If we have a result, close the window
        if self.result.is_some() {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            // ── Top row: TabBar + theme ComboBox ──────────────────────────
            ui.horizontal(|ui| {
                // Sync tab_idx from tab enum
                self.tab_idx = match self.tab {
                    MainTab::Saved => 0,
                    MainTab::Manual => 1,
                    MainTab::Accounts => 2,
                };

                let before = self.tab_idx;
                egui_theme::TabBar::new(
                    &mut self.tab_idx,
                    &["Saved Entry", "Manual Entry", "Account Mgmt"],
                )
                .show(ui);

                if self.tab_idx != before {
                    self.tab = match self.tab_idx {
                        0 => MainTab::Saved,
                        1 => MainTab::Manual,
                        _ => MainTab::Accounts,
                    };
                }

                // Push theme ComboBox to the right
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let themes = ["Slate", "Ember", "Fantasy"];
                    let current_idx = match self.theme_config.theme.as_str() {
                        "ember" => 1,
                        "fantasy" => 2,
                        _ => 0,
                    };
                    let mut selected = current_idx;
                    egui::ComboBox::from_id_salt("theme_selector")
                        .width(80.0)
                        .selected_text(themes[selected])
                        .show_ui(ui, |ui| {
                            for (i, name) in themes.iter().enumerate() {
                                if ui.selectable_value(&mut selected, i, *name).clicked() {
                                    let key = match i {
                                        1 => "ember",
                                        2 => "fantasy",
                                        _ => "slate",
                                    };
                                    self.theme_config.theme = key.to_string();
                                    self.theme_config.save();
                                    self.theme_config.to_theme().apply(ui.ctx());
                                }
                            }
                        });
                });
            });

            ui.add_space(4.0);

            match self.tab {
                MainTab::Saved => self.show_saved_tab(ui),
                MainTab::Manual => self.show_manual_tab(ui, ctx),
                MainTab::Accounts => self.show_accounts_tab(ui, ctx),
            }
        });
    }
}

// ─── Saved Entry tab ──────────────────────────────────────────────────────────

impl LoginApp {
    fn show_saved_tab(&mut self, ui: &mut egui::Ui) {
        if self.store.accounts.is_empty() {
            ui.label("You have no saved login info.");
            return;
        }

        let palette = egui_theme::palette_from_ctx(ui.ctx());
        let authenticating = self.play_state == PlayState::Authenticating;
        let mut play_pending: Option<PendingPlay> = None;
        let mut toggle_fav: Option<(String, String, String)> = None;
        let mut remove_char: Option<(String, String, String)> = None;

        // Build tab labels: FAVORITES (if any) + account names UPPERCASED
        let has_favorites = self
            .store
            .accounts
            .iter()
            .any(|a| a.characters.iter().any(|c| c.favorite));

        let mut tab_labels: Vec<String> = Vec::new();
        if has_favorites {
            tab_labels.push("\u{2605} FAVORITES".to_string());
        }
        for acct in &self.store.accounts {
            tab_labels.push(acct.account.to_uppercase());
        }

        let tab_label_refs: Vec<&str> = tab_labels.iter().map(|s| s.as_str()).collect();

        // Clamp saved_side_tab
        if self.saved_side_tab >= tab_label_refs.len() {
            self.saved_side_tab = 0;
        }

        egui_theme::SideTabBar::new("saved_tabs", &mut self.saved_side_tab, &tab_label_refs)
            .tab_width(130.0)
            .show(ui, |ui, selected_idx| {
                let is_favorites_tab = has_favorites && selected_idx == 0;
                let account_idx = if has_favorites {
                    if selected_idx == 0 { None } else { Some(selected_idx - 1) }
                } else {
                    Some(selected_idx)
                };

                if is_favorites_tab {
                    // ── Favorites view ────────────────────────────────────
                    let mut favorites = Vec::new();
                    for acct in &self.store.accounts {
                        for ch in &acct.characters {
                            if ch.favorite {
                                favorites.push((
                                    acct.account.clone(),
                                    ch.name.clone(),
                                    ch.game_code.clone(),
                                    ch.game_name.clone(),
                                    ch.frontend.clone(),
                                    ch.custom_launch.clone(),
                                    ch.custom_launch_dir.clone(),
                                ));
                            }
                        }
                    }
                    favorites.sort_by(|a, b| a.1.to_lowercase().cmp(&b.1.to_lowercase()));

                    if favorites.is_empty() {
                        ui.label("No favorite characters yet.");
                    } else {
                        egui::ScrollArea::vertical().show(ui, |ui| {
                            let mut last_game: Option<String> = None;
                            for (account, name, game_code, game_name, frontend_str, custom_launch, custom_launch_dir) in &favorites {
                                // Game separator
                                if last_game.as_ref() != Some(game_code) {
                                    if last_game.is_some() {
                                        ui.add_space(2.0);
                                        egui_theme::ThemedSeparator::labeled(game_name.as_str()).show(ui);
                                        ui.add_space(2.0);
                                    } else {
                                        egui_theme::ThemedSeparator::labeled(game_name.as_str()).show(ui);
                                        ui.add_space(2.0);
                                    }
                                    last_game = Some(game_code.clone());
                                }

                                ui.horizontal(|ui| {
                                    let frontend_display = Frontend::from_str(frontend_str).display_name();
                                    let play_label = format!(
                                        "\u{25B6} {}    {}    [{}]",
                                        name, game_name, frontend_display
                                    );
                                    if ui
                                        .add_enabled(
                                            !authenticating,
                                            egui::Button::new(&play_label)
                                                .min_size(egui::vec2(280.0, 24.0)),
                                        )
                                        .clicked()
                                    {
                                        match self.store.get_password(account, &self.key) {
                                            Ok(pw) => {
                                                play_pending = Some(PendingPlay {
                                                    account: account.clone(),
                                                    password: pw,
                                                    game_code: game_code.clone(),
                                                    character: name.clone(),
                                                    frontend: Frontend::from_str(frontend_str),
                                                    custom_launch: custom_launch.clone(),
                                                    custom_launch_dir: custom_launch_dir.clone(),
                                                });
                                            }
                                            Err(e) => {
                                                self.saved_status =
                                                    format!("Failed to decrypt password: {e}");
                                            }
                                        }
                                    }
                                    // Star toggle (gold for favorite)
                                    let star_text = egui::RichText::new("\u{2605}")
                                        .color(egui::Color32::from_rgb(218, 165, 32));
                                    if ui.button(star_text).clicked() {
                                        toggle_fav = Some((
                                            account.clone(),
                                            name.clone(),
                                            game_code.clone(),
                                        ));
                                    }
                                    // Remove button (red)
                                    if ui
                                        .add(egui::Button::new(
                                            egui::RichText::new("Remove")
                                                .color(palette.error)
                                                .small(),
                                        ))
                                        .clicked()
                                    {
                                        remove_char = Some((
                                            account.clone(),
                                            name.clone(),
                                            game_code.clone(),
                                        ));
                                    }
                                });
                            }
                        });
                    }
                } else if let Some(acct_idx) = account_idx {
                    // ── Account characters view ──────────────────────────
                    if let Some(acct) = self.store.accounts.get(acct_idx) {
                        let account_name = acct.account.clone();
                        let chars: Vec<_> = acct
                            .characters
                            .iter()
                            .map(|c| {
                                (
                                    c.name.clone(),
                                    c.game_code.clone(),
                                    c.game_name.clone(),
                                    c.favorite,
                                    c.frontend.clone(),
                                    c.custom_launch.clone(),
                                    c.custom_launch_dir.clone(),
                                )
                            })
                            .collect();

                        if chars.is_empty() {
                            ui.label("No characters saved for this account.");
                        } else {
                            egui::ScrollArea::vertical().show(ui, |ui| {
                                let mut last_game: Option<String> = None;
                                for (name, game_code, game_name, is_fav, frontend_str, custom_launch, custom_launch_dir) in &chars {
                                    // Game separator
                                    if last_game.as_ref() != Some(game_code) {
                                        if last_game.is_some() {
                                            ui.add_space(2.0);
                                        }
                                        egui_theme::ThemedSeparator::labeled(game_name.as_str()).show(ui);
                                        ui.add_space(2.0);
                                        last_game = Some(game_code.clone());
                                    }
                                    ui.horizontal(|ui| {
                                        let frontend_display = Frontend::from_str(frontend_str).display_name();
                                        let play_label = format!(
                                            "\u{25B6} {}    {}    [{}]",
                                            name, game_name, frontend_display
                                        );
                                        if ui
                                            .add_enabled(
                                                !authenticating,
                                                egui::Button::new(&play_label)
                                                    .min_size(egui::vec2(280.0, 24.0)),
                                            )
                                            .clicked()
                                        {
                                            match self
                                                .store
                                                .get_password(&account_name, &self.key)
                                            {
                                                Ok(pw) => {
                                                    play_pending = Some(PendingPlay {
                                                        account: account_name.clone(),
                                                        password: pw,
                                                        game_code: game_code.clone(),
                                                        character: name.clone(),
                                                        frontend: Frontend::from_str(frontend_str),
                                                        custom_launch: custom_launch.clone(),
                                                        custom_launch_dir: custom_launch_dir.clone(),
                                                    });
                                                }
                                                Err(e) => {
                                                    self.saved_status = format!(
                                                        "Failed to decrypt password: {e}"
                                                    );
                                                }
                                            }
                                        }
                                        let star = if *is_fav { "\u{2605}" } else { "\u{2606}" };
                                        let star_color = if *is_fav {
                                            egui::Color32::from_rgb(218, 165, 32)
                                        } else {
                                            palette.text_secondary
                                        };
                                        if ui.button(egui::RichText::new(star).color(star_color)).clicked() {
                                            toggle_fav = Some((
                                                account_name.clone(),
                                                name.clone(),
                                                game_code.clone(),
                                            ));
                                        }
                                        if ui
                                            .add(egui::Button::new(
                                                egui::RichText::new("Remove")
                                                    .color(palette.error)
                                                    .small(),
                                            ))
                                            .clicked()
                                        {
                                            remove_char = Some((
                                                account_name.clone(),
                                                name.clone(),
                                                game_code.clone(),
                                            ));
                                        }
                                    });
                                }
                            });
                        }
                    } else {
                        ui.label("Select an account.");
                    }
                }
            });

        // Handle deferred mutations
        if let Some((acct, name, game_code)) = toggle_fav {
            self.store.toggle_favorite(&acct, &name, &game_code);
            let _ = self.store.save();
        }
        if let Some((acct, name, _game_code)) = remove_char {
            if let Some(a) = self
                .store
                .accounts
                .iter_mut()
                .find(|a| a.account == acct)
            {
                a.characters
                    .retain(|c| c.name.to_lowercase() != name.to_lowercase());
            }
            let _ = self.store.save();
        }
        if let Some(pending) = play_pending {
            self.saved_status = "Authenticating...".to_string();
            self.start_play(pending);
        }

        // Status + Refresh at bottom
        ui.add_space(4.0);
        if ui.button("Refresh Entries").clicked() {
            self.store = CredentialStore::load().unwrap_or_default();
            self.saved_status.clear();
        }
        if !self.saved_status.is_empty() {
            let color = if self.saved_status.starts_with("Auth failed")
                || self.saved_status.starts_with("Failed")
                || self.saved_status.starts_with("Error")
            {
                palette.error
            } else {
                palette.success
            };
            ui.colored_label(color, &self.saved_status);
        }
    }
}

// ─── Manual Entry tab ─────────────────────────────────────────────────────────

impl LoginApp {
    fn show_manual_tab(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let palette = egui_theme::palette_from_ctx(ui.ctx());
        let is_fetching = self.connect_state == ConnectState::Fetching;
        let is_connected = matches!(self.connect_state, ConnectState::Connected(_));
        let authenticating = self.play_state == PlayState::Authenticating;

        // Login fields
        let mut trigger_connect = false;
        egui::Grid::new("manual_grid")
            .num_columns(2)
            .spacing([12.0, 6.0])
            .show(ui, |ui| {
                ui.label("User ID:");
                let r = ui.add_enabled(
                    !is_fetching,
                    egui::TextEdit::singleline(&mut self.manual_account),
                );
                if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    trigger_connect = true;
                }
                ui.end_row();

                ui.label("Password:");
                let r = ui.add_enabled(
                    !is_fetching,
                    egui::TextEdit::singleline(&mut self.manual_password).password(true),
                );
                if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    trigger_connect = true;
                }
                ui.end_row();

                ui.label("Game:");
                egui::ComboBox::from_id_salt("manual_game")
                    .selected_text(GAME_CODES[self.manual_game_idx.min(GAME_CODES.len() - 1)].1)
                    .show_ui(ui, |ui| {
                        for (i, &(code, name)) in GAME_CODES.iter().enumerate() {
                            if ui.selectable_value(&mut self.manual_game_idx, i, name).clicked() {
                                self.manual_game_code = code.to_string();
                            }
                        }
                    });
                ui.end_row();
            });

        ui.add_space(6.0);

        // Connect / Disconnect buttons
        ui.horizontal(|ui| {
            if ui
                .add_enabled(
                    !is_fetching && !is_connected,
                    egui::Button::new("Connect"),
                )
                .clicked()
                || trigger_connect
            {
                self.start_manual_fetch(ctx);
            }
            if ui
                .add_enabled(is_connected, egui::Button::new("Disconnect"))
                .clicked()
            {
                self.connect_state = ConnectState::Idle;
                self.manual_selected_char = None;
                self.manual_tree_selected = None;
                self.manual_status.clear();
            }
        });

        ui.add_space(6.0);

        // Character list — TreeView
        let columns = vec![
            egui_theme::TreeColumn { label: "Game".into(), width: Some(140.0), sortable: false },
            egui_theme::TreeColumn { label: "Character".into(), width: None, sortable: false },
        ];

        egui::ScrollArea::vertical()
            .max_height(160.0)
            .show(ui, |ui| {
                match &self.connect_state {
                    ConnectState::Idle => {
                        ui.label("");
                    }
                    ConnectState::Fetching => {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.label("Connecting...");
                        });
                    }
                    ConnectState::Connected(chars) => {
                        let mut tree_rows: Vec<egui_theme::TreeRow> = chars
                            .iter()
                            .map(|ch| egui_theme::TreeRow {
                                cells: vec![
                                    Self::game_name(&self.manual_game_code).to_string(),
                                    ch.name.clone(),
                                ],
                                children: vec![],
                                expanded: false,
                            })
                            .collect();

                        let resp = egui_theme::TreeView::new(
                            "manual_chars",
                            &columns,
                            &mut tree_rows,
                            &mut self.manual_tree_selected,
                        )
                        .show(ui);

                        if resp.clicked_row.is_some() {
                            self.manual_selected_char = self.manual_tree_selected;
                        }
                    }
                }
            });

        ui.add_space(6.0);

        // Frontend selection
        ui.horizontal(|ui| {
            ui.label("Frontend:");
            ui.radio_value(&mut self.manual_frontend, Frontend::Wrayth, "Wrayth");
            ui.radio_value(&mut self.manual_frontend, Frontend::Wizard, "Wizard");
            ui.radio_value(&mut self.manual_frontend, Frontend::Avalon, "Avalon");
        });

        ui.checkbox(&mut self.manual_custom_launch_enabled, "Custom launch command");
        if self.manual_custom_launch_enabled {
            let cmd_options = launch_cmd_suggestions();
            let dir_options = launch_dir_suggestions();
            ui.horizontal(|ui| {
                ui.label("Command:");
                egui_theme::EditableComboBox::new(
                    "manual_launch_cmd",
                    &mut self.manual_custom_launch,
                    &cmd_options,
                )
                .hint_text("Custom command...")
                .show(ui);
            });
            ui.horizontal(|ui| {
                ui.label("Directory:");
                egui_theme::EditableComboBox::new(
                    "manual_launch_dir",
                    &mut self.manual_custom_launch_dir,
                    &dir_options,
                )
                .hint_text("Working directory...")
                .show(ui);
            });
        }

        ui.add_space(6.0);
        ui.checkbox(&mut self.manual_save, "Save this info for quick game entry");
        if self.manual_save {
            ui.checkbox(&mut self.manual_favorite, "\u{2605} Mark as favorite");
        }

        ui.add_space(6.0);

        // Play button — right-aligned
        let can_play = self.manual_selected_char.is_some() && is_connected && !authenticating;
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Max), |ui| {
            if ui
                .add_enabled(can_play, egui::Button::new("Play"))
                .clicked()
            {
                self.manual_play();
            }
        });

        // Status
        if !self.manual_status.is_empty() {
            let color = if self.manual_status.starts_with("Error")
                || self.manual_status.starts_with("Auth failed")
                || self.manual_status.starts_with("Account")
            {
                palette.error
            } else {
                palette.success
            };
            ui.colored_label(color, &self.manual_status);
        }
    }

    fn start_manual_fetch(&mut self, ctx: &egui::Context) {
        if self.manual_account.is_empty() {
            self.manual_status = "Account is required.".to_string();
            return;
        }
        self.connect_state = ConnectState::Fetching;
        self.manual_status.clear();
        self.manual_selected_char = None;
        self.manual_tree_selected = None;

        let account = self.manual_account.clone();
        let password = self.manual_password.clone();
        let game_code = self.manual_game_code.clone();
        let tx = self.fetch_tx.clone();
        let ctx = ctx.clone();

        std::thread::spawn(move || {
            let rt = match tokio::runtime::Runtime::new() {
                Ok(rt) => rt,
                Err(e) => {
                    let _ = tx.send(Err(e.to_string()));
                    ctx.request_repaint();
                    return;
                }
            };
            let result = rt.block_on(list_characters(&account, &password, &game_code));
            let _ = tx.send(result.map_err(|e| format!("{e:#}")));
            ctx.request_repaint();
        });
    }

    fn manual_play(&mut self) {
        if let ConnectState::Connected(ref chars) = self.connect_state {
            if let Some(idx) = self.manual_selected_char {
                if let Some(ch) = chars.get(idx) {
                    let account = self.manual_account.clone();
                    let password = self.manual_password.clone();
                    let game_code = self.manual_game_code.clone();
                    let character = ch.name.clone();

                    let custom_launch = if self.manual_custom_launch_enabled {
                        Some(self.manual_custom_launch.clone())
                    } else {
                        None
                    };
                    let custom_launch_dir = if self.manual_custom_launch_enabled {
                        Some(self.manual_custom_launch_dir.clone())
                    } else {
                        None
                    };

                    if self.manual_save {
                        // Ensure account exists
                        let exists = self
                            .store
                            .accounts
                            .iter()
                            .any(|a| a.account.to_lowercase() == account.to_lowercase());
                        if !exists {
                            if let Err(e) =
                                self.store.add_account(&account, &password, &self.key)
                            {
                                self.manual_status =
                                    format!("Failed to save account: {e}");
                                return;
                            }
                        }
                        self.store.add_character(
                            &account,
                            &character,
                            &game_code,
                            Self::game_name(&game_code),
                            self.manual_frontend.as_str(),
                            custom_launch.clone(),
                            custom_launch_dir.clone(),
                        );
                        let _ = self.store.save();
                    }

                    let pending = PendingPlay {
                        account,
                        password,
                        game_code,
                        character,
                        frontend: self.manual_frontend.clone(),
                        custom_launch,
                        custom_launch_dir,
                    };
                    self.manual_status = "Authenticating...".to_string();
                    self.start_play(pending);
                }
            }
        }
    }
}

// ─── Account Management tab ───────────────────────────────────────────────────

impl LoginApp {
    fn show_accounts_tab(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        // Inner TabBar for 4 sub-tabs
        self.acct_sub_tab_idx = match self.acct_sub_tab {
            AcctSubTab::Accounts => 0,
            AcctSubTab::AddCharacter => 1,
            AcctSubTab::AddAccount => 2,
            AcctSubTab::Encryption => 3,
        };

        let before = self.acct_sub_tab_idx;
        egui_theme::TabBar::new(
            &mut self.acct_sub_tab_idx,
            &["Accounts", "Add Character", "Add Account", "Encryption"],
        )
        .show(ui);

        if self.acct_sub_tab_idx != before {
            self.acct_sub_tab = match self.acct_sub_tab_idx {
                0 => AcctSubTab::Accounts,
                1 => AcctSubTab::AddCharacter,
                2 => AcctSubTab::AddAccount,
                _ => AcctSubTab::Encryption,
            };
        }

        ui.add_space(8.0);

        match self.acct_sub_tab {
            AcctSubTab::Accounts => self.show_acct_accounts_sub(ui),
            AcctSubTab::AddCharacter => self.show_acct_add_char_sub(ui),
            AcctSubTab::AddAccount => self.show_acct_add_account_sub(ui, ctx),
            AcctSubTab::Encryption => self.show_encryption_management(ui),
        }
    }

    fn show_acct_accounts_sub(&mut self, ui: &mut egui::Ui) {
        let palette = egui_theme::palette_from_ctx(ui.ctx());

        if self.store.accounts.is_empty() {
            ui.label("No saved accounts.");
            return;
        }

        // Build hierarchical TreeView data
        let columns = vec![
            egui_theme::TreeColumn { label: "Account".into(), width: Some(120.0), sortable: true },
            egui_theme::TreeColumn { label: "Character".into(), width: Some(120.0), sortable: true },
            egui_theme::TreeColumn { label: "Game".into(), width: Some(120.0), sortable: false },
            egui_theme::TreeColumn { label: "Frontend".into(), width: Some(80.0), sortable: false },
            egui_theme::TreeColumn { label: "Fav".into(), width: Some(40.0), sortable: false },
        ];

        let mut tree_rows: Vec<egui_theme::TreeRow> = self
            .store
            .accounts
            .iter()
            .map(|acct| {
                let children: Vec<egui_theme::TreeRow> = acct
                    .characters
                    .iter()
                    .map(|ch| {
                        let fav_str = if ch.favorite { "\u{2605}" } else { "\u{2606}" };
                        let frontend_display = Frontend::from_str(&ch.frontend).display_name();
                        egui_theme::TreeRow {
                            cells: vec![
                                String::new(),
                                ch.name.clone(),
                                ch.game_name.clone(),
                                frontend_display.to_string(),
                                fav_str.to_string(),
                            ],
                            children: vec![],
                            expanded: false,
                        }
                    })
                    .collect();
                egui_theme::TreeRow {
                    cells: vec![
                        acct.account.to_uppercase(),
                        String::new(),
                        String::new(),
                        String::new(),
                        String::new(),
                    ],
                    children,
                    expanded: true,
                }
            })
            .collect();

        egui::ScrollArea::vertical().max_height(300.0).show(ui, |ui| {
            egui_theme::TreeView::new(
                "acct_tree",
                &columns,
                &mut tree_rows,
                &mut self.acct_tree_selected,
            )
            .sort_state(&mut self.acct_tree_sort_col, &mut self.acct_tree_sort_asc)
            .show(ui);
        });

        ui.add_space(8.0);

        // Button row
        let mut remove_account: Option<String> = None;

        ui.horizontal(|ui| {
            if ui.button("Refresh").clicked() {
                self.store = CredentialStore::load().unwrap_or_default();
                self.accounts_status.clear();
            }

            if ui
                .add(egui::Button::new(
                    egui::RichText::new("Remove Account").color(palette.error),
                ))
                .clicked()
            {
                // Find which account is selected (top-level row)
                if let Some(sel) = self.acct_tree_selected {
                    if sel < self.store.accounts.len() {
                        remove_account = Some(self.store.accounts[sel].account.clone());
                    }
                }
            }

            if ui.button("Add Account").clicked() {
                self.acct_sub_tab = AcctSubTab::AddAccount;
                self.acct_sub_tab_idx = 2;
            }
        });

        // Change password inline
        if let Some(ref pw_acct) = self.change_pw_account.clone() {
            ui.horizontal(|ui| {
                ui.label("New password:");
                ui.add(
                    egui::TextEdit::singleline(&mut self.change_pw_value).password(true),
                );
                if ui.button("Save").clicked() {
                    if let Err(e) = self.store.add_account(
                        pw_acct,
                        &self.change_pw_value,
                        &self.key,
                    ) {
                        self.accounts_status = format!("Error: {e}");
                    } else {
                        let _ = self.store.save();
                        self.accounts_status =
                            format!("Password updated for '{pw_acct}'.");
                    }
                    self.change_pw_account = None;
                    self.change_pw_value.clear();
                }
                if ui.button("Cancel").clicked() {
                    self.change_pw_account = None;
                    self.change_pw_value.clear();
                }
            });
        }

        // Handle removal
        if let Some(acct) = remove_account {
            self.store.remove_account(&acct);
            let _ = self.store.save();
            self.accounts_status = format!("Removed account '{acct}'.");
            self.acct_tree_selected = None;
        }

        if !self.accounts_status.is_empty() {
            let color = if self.accounts_status.starts_with("Error")
                || self.accounts_status.starts_with("Removed")
            {
                palette.text_primary
            } else {
                palette.success
            };
            ui.colored_label(color, &self.accounts_status);
        }
    }

    fn show_acct_add_char_sub(&mut self, ui: &mut egui::Ui) {
        let palette = egui_theme::palette_from_ctx(ui.ctx());

        if self.store.accounts.is_empty() {
            ui.label("Add an account first.");
            return;
        }

        let account_names: Vec<String> =
            self.store.accounts.iter().map(|a| a.account.clone()).collect();

        // Clamp index
        if self.add_char_account_idx >= account_names.len() {
            self.add_char_account_idx = 0;
        }

        egui::Grid::new("add_char_grid")
            .num_columns(2)
            .spacing([12.0, 6.0])
            .show(ui, |ui| {
                ui.label("Account:");
                egui::ComboBox::from_id_salt("add_char_acct")
                    .selected_text(&account_names[self.add_char_account_idx])
                    .show_ui(ui, |ui| {
                        for (i, name) in account_names.iter().enumerate() {
                            ui.selectable_value(&mut self.add_char_account_idx, i, name);
                        }
                    });
                ui.end_row();

                ui.label("Character:");
                ui.text_edit_singleline(&mut self.add_char_name);
                ui.end_row();

                ui.label("Game:");
                egui::ComboBox::from_id_salt("add_char_game")
                    .selected_text(GAME_CODES[self.add_char_game_idx.min(GAME_CODES.len() - 1)].1)
                    .show_ui(ui, |ui| {
                        for (i, &(_code, name)) in GAME_CODES.iter().enumerate() {
                            ui.selectable_value(&mut self.add_char_game_idx, i, name);
                        }
                    });
                ui.end_row();
            });

        ui.add_space(6.0);

        ui.horizontal(|ui| {
            ui.label("Frontend:");
            ui.radio_value(&mut self.add_char_frontend, Frontend::Wrayth, "Wrayth");
            ui.radio_value(&mut self.add_char_frontend, Frontend::Wizard, "Wizard");
            ui.radio_value(&mut self.add_char_frontend, Frontend::Avalon, "Avalon");
        });

        ui.checkbox(&mut self.add_char_custom_launch_enabled, "Custom launch command");
        if self.add_char_custom_launch_enabled {
            let cmd_options = launch_cmd_suggestions();
            let dir_options = launch_dir_suggestions();
            ui.horizontal(|ui| {
                ui.label("Command:");
                egui_theme::EditableComboBox::new(
                    "add_char_launch_cmd",
                    &mut self.add_char_custom_launch,
                    &cmd_options,
                )
                .hint_text("Custom command...")
                .show(ui);
            });
            ui.horizontal(|ui| {
                ui.label("Directory:");
                egui_theme::EditableComboBox::new(
                    "add_char_launch_dir",
                    &mut self.add_char_custom_launch_dir,
                    &dir_options,
                )
                .hint_text("Working directory...")
                .show(ui);
            });
        }

        ui.add_space(6.0);

        // Add Character button — right-aligned
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Max), |ui| {
            if ui.button("Add Character").clicked() {
                if self.add_char_name.trim().is_empty() {
                    self.add_char_status = "Character name is required.".to_string();
                } else {
                    let acct = &account_names[self.add_char_account_idx];
                    let game_idx = self.add_char_game_idx.min(GAME_CODES.len() - 1);
                    let (code, name) = GAME_CODES[game_idx];
                    let custom_launch = if self.add_char_custom_launch_enabled {
                        Some(self.add_char_custom_launch.clone())
                    } else {
                        None
                    };
                    let custom_launch_dir = if self.add_char_custom_launch_enabled {
                        Some(self.add_char_custom_launch_dir.clone())
                    } else {
                        None
                    };
                    self.store.add_character(
                        acct,
                        self.add_char_name.trim(),
                        code,
                        name,
                        self.add_char_frontend.as_str(),
                        custom_launch,
                        custom_launch_dir,
                    );
                    match self.store.save() {
                        Ok(()) => {
                            self.add_char_status = format!(
                                "Added '{}' to '{}'.",
                                self.add_char_name.trim(),
                                acct
                            );
                            self.add_char_name.clear();
                        }
                        Err(e) => {
                            self.add_char_status = format!("Failed to save: {e}");
                        }
                    }
                }
            }
        });

        if !self.add_char_status.is_empty() {
            let color = if self.add_char_status.starts_with("Failed")
                || self.add_char_status.starts_with("Character name")
            {
                palette.error
            } else {
                palette.success
            };
            ui.colored_label(color, &self.add_char_status);
        }
    }

    fn show_acct_add_account_sub(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let palette = egui_theme::palette_from_ctx(ui.ctx());
        let mut trigger_connect = false;

        egui::Grid::new("add_acct_grid")
            .num_columns(2)
            .spacing([12.0, 6.0])
            .show(ui, |ui| {
                ui.label("Username:");
                let r = ui.text_edit_singleline(&mut self.add_acct_username);
                if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    trigger_connect = true;
                }
                ui.end_row();

                ui.label("Password:");
                let r = ui.add(
                    egui::TextEdit::singleline(&mut self.add_acct_password)
                        .password(!self.add_acct_show_password),
                );
                if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    trigger_connect = true;
                }
                ui.end_row();
            });

        ui.checkbox(&mut self.add_acct_show_password, "Show password");
        ui.add_space(6.0);

        // Connect button
        let connect_clicked = ui
            .add_enabled(!self.add_acct_fetching, egui::Button::new("Connect"))
            .clicked();

        if connect_clicked || (trigger_connect && !self.add_acct_fetching) {
            if self.add_acct_username.is_empty() {
                self.add_acct_status = "Username is required.".to_string();
            } else {
                self.add_acct_fetching = true;
                self.add_acct_status = "Fetching characters...".to_string();
                self.add_acct_chars.clear();
                self.add_acct_tree_selected = None;

                let account = self.add_acct_username.clone();
                let password = self.add_acct_password.clone();
                let tx = self.add_acct_tx.clone();
                let ctx = ctx.clone();

                std::thread::spawn(move || {
                    let rt = match tokio::runtime::Runtime::new() {
                        Ok(rt) => rt,
                        Err(e) => {
                            let _ = tx.send(Err(e.to_string()));
                            ctx.request_repaint();
                            return;
                        }
                    };
                    let result = rt.block_on(list_characters(&account, &password, "GS3"));
                    let _ = tx.send(result.map_err(|e| format!("{e:#}")));
                    ctx.request_repaint();
                });
            }
        }

        if self.add_acct_fetching {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label("Connecting...");
            });
        }

        // TreeView for fetched characters
        if !self.add_acct_chars.is_empty() {
            ui.add_space(6.0);

            let columns = vec![
                egui_theme::TreeColumn { label: "Game".into(), width: Some(140.0), sortable: false },
                egui_theme::TreeColumn { label: "Character".into(), width: None, sortable: false },
            ];

            let mut tree_rows: Vec<egui_theme::TreeRow> = self
                .add_acct_chars
                .iter()
                .map(|ch| egui_theme::TreeRow {
                    cells: vec!["GemStone IV".to_string(), ch.name.clone()],
                    children: vec![],
                    expanded: false,
                })
                .collect();

            egui::ScrollArea::vertical().max_height(160.0).show(ui, |ui| {
                egui_theme::TreeView::new(
                    "add_acct_chars",
                    &columns,
                    &mut tree_rows,
                    &mut self.add_acct_tree_selected,
                )
                .show(ui);
            });

            ui.add_space(6.0);

            // Add Account button — right-aligned
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Max), |ui| {
                if ui.button("Add Account").clicked() {
                    let account = self.add_acct_username.clone();
                    let password = self.add_acct_password.clone();
                    if account.is_empty() {
                        self.add_acct_status = "Username is required.".to_string();
                    } else {
                        let exists = self
                            .store
                            .accounts
                            .iter()
                            .any(|a| a.account.to_lowercase() == account.to_lowercase());
                        if !exists {
                            if let Err(e) = self.store.add_account(&account, &password, &self.key) {
                                self.add_acct_status = format!("Failed to save account: {e}");
                                return;
                            }
                        }
                        for ch in &self.add_acct_chars {
                            self.store.add_character(
                                &account,
                                &ch.name,
                                "GS3",
                                Self::game_name("GS3"),
                                "stormfront",
                                None,
                                None,
                            );
                        }
                        match self.store.save() {
                            Ok(()) => {
                                self.add_acct_status = format!(
                                    "Saved {} character(s) for '{account}'.",
                                    self.add_acct_chars.len()
                                );
                                self.add_acct_chars.clear();
                                self.add_acct_tree_selected = None;
                            }
                            Err(e) => {
                                self.add_acct_status = format!("Failed to save: {e}");
                            }
                        }
                    }
                }
            });
        }

        if !self.add_acct_status.is_empty() {
            let color = if self.add_acct_status.starts_with("Error")
                || self.add_acct_status.starts_with("Failed")
                || self.add_acct_status.starts_with("Username")
            {
                palette.error
            } else {
                palette.success
            };
            ui.colored_label(color, &self.add_acct_status);
        }
    }

    fn show_encryption_management(&mut self, ui: &mut egui::Ui) {
        ui.add_space(8.0);
        ui.label("Current encryption mode: AES-256-GCM (local key)");
        ui.add_space(8.0);
        ui.add_enabled(false, egui::Button::new("Change Mode"))
            .on_disabled_hover_text("Coming in a future update");
    }
}
