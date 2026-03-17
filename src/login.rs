#![cfg(feature = "login-gui")]

use crate::credentials::CredentialStore;
use crate::eaccess::{list_all_characters, list_characters, CharacterEntry};
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
/// Custom launch command suggestions — matches lich-5's login_tab_utils.rb
fn launch_cmd_suggestions() -> Vec<String> {
    let mut suggestions = vec![
        "Wizard.Exe /GGS /H127.0.0.1 /P%port% /K%key%".into(),
        "Stormfront.exe /GGS /Hlocalhost /P%port% /K%key%".into(),
    ];
    if cfg!(target_os = "macos") {
        suggestions.push(
            "/Applications/Warlock.app/Contents/MacOS/Warlock --host localhost --port %port% --key %key%".into()
        );
    }
    if cfg!(target_os = "windows") {
        suggestions.push(
            "warlock --host localhost --port %port% --key %key%".into()
        );
    }
    if cfg!(target_os = "linux") {
        suggestions.push(
            "/usr/bin/warlock --host localhost --port %port% --key %key%".into()
        );
    }
    suggestions
}

/// Custom launch directory suggestions — matches lich-5's login_tab_utils.rb
fn launch_dir_suggestions() -> Vec<String> {
    vec![
        "../wizard".into(),
        "../StormFront".into(),
    ]
}

pub struct LoginApp {
    // Tab selection
    tab: MainTab,
    tab_idx: usize,

    // ── Saved Entry tab ───────────────────────────────────────────────
    store: CredentialStore,
    key: Option<[u8; 32]>,
    saved_side_tab: usize,
    saved_status: String,

    // ── Manual Entry tab ──────────────────────────────────────────────
    manual_account: String,
    manual_password: String,
    connect_state: ConnectState,
    manual_selected_char: Option<usize>,
    manual_tree_selected: Option<usize>,
    manual_tree_sort_col: Option<usize>,
    manual_tree_sort_asc: bool,
    manual_save: bool,
    manual_favorite: bool,
    manual_status: String,
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
    change_pw_current: String,
    change_pw_new: String,
    change_pw_confirm: String,
    change_pw_status: String,
    change_pw_verifying: bool,
    change_pw_tx: SyncSender<Result<String, String>>,  // Ok("new"|"old") or Err(msg)
    change_pw_rx: Receiver<Result<String, String>>,
    acct_tree_selected: Option<usize>,
    acct_tree_sort_col: Option<usize>,
    acct_tree_sort_asc: bool,
    acct_tree_expanded: std::collections::HashMap<String, bool>,
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
    app_config: crate::app_config::AppConfig,
    theme_applied: bool,

    // ── Encryption ────────────────────────────────────────────────────
    enc_config: crate::encryption::EncryptionConfig,
    master_password_prompt: bool,
    master_password_input: String,
    master_password_error: String,
    // Encryption management dialog state
    enc_new_mode: crate::encryption::EncryptionMode,
    enc_new_password: String,
    enc_confirm_password: String,
    enc_current_password: String,
    enc_status: String,
    enc_show_dialog: bool,

    // ── Result ────────────────────────────────────────────────────────
    pub result: Option<LoginResult>,
}

impl LoginApp {
    pub fn new() -> Self {
        let (fetch_tx, fetch_rx) = sync_channel(1);
        let (add_acct_tx, add_acct_rx) = sync_channel(1);
        let (play_tx, play_rx) = sync_channel(1);
        let (change_pw_tx, change_pw_rx) = sync_channel::<Result<String, String>>(1);
        let enc_config = crate::encryption::EncryptionConfig::load();
        let key = match enc_config.mode {
            crate::encryption::EncryptionMode::Plaintext => None,
            crate::encryption::EncryptionMode::Standard => {
                Some(CredentialStore::load_or_create_key().unwrap_or([0u8; 32]))
            }
            crate::encryption::EncryptionMode::Enhanced => {
                crate::encryption::get_key_from_keychain().ok().flatten()
            }
        };
        let master_password_prompt =
            enc_config.mode == crate::encryption::EncryptionMode::Enhanced && key.is_none();
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
            manual_tree_sort_col: Some(0),
            manual_tree_sort_asc: true,
            manual_save: false,
            manual_favorite: false,
            manual_status: String::new(),
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
            change_pw_current: String::new(),
            change_pw_new: String::new(),
            change_pw_confirm: String::new(),
            change_pw_status: String::new(),
            change_pw_verifying: false,
            change_pw_tx: change_pw_tx,
            change_pw_rx: change_pw_rx,
            acct_tree_selected: None,
            acct_tree_sort_col: None,
            acct_tree_sort_asc: true,
            acct_tree_expanded: std::collections::HashMap::new(),
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
            app_config: crate::app_config::AppConfig::load(),
            theme_applied: false,
            enc_new_mode: enc_config.mode.clone(),
            enc_config,
            master_password_prompt,
            master_password_input: String::new(),
            master_password_error: String::new(),
            enc_new_password: String::new(),
            enc_confirm_password: String::new(),
            enc_current_password: String::new(),
            enc_status: String::new(),
            enc_show_dialog: false,
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
            self.app_config.to_theme().apply(ctx);
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
                            theme: self.app_config.theme.clone(),
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

        // Poll change-password verification result
        if let Ok(res) = self.change_pw_rx.try_recv() {
            self.change_pw_verifying = false;
            match res {
                Ok(which) => {
                    if which == "new" {
                        // New password works on server — save it locally
                        if let Some(ref pw_acct) = self.change_pw_account.clone() {
                            match self.store.add_account(pw_acct, &self.change_pw_new, self.key.as_ref()) {
                                Ok(()) => {
                                    let _ = self.store.save();
                                    self.accounts_status = format!("Password changed successfully for '{pw_acct}'.");
                                    self.change_pw_account = None;
                                    self.change_pw_current.clear();
                                    self.change_pw_new.clear();
                                    self.change_pw_confirm.clear();
                                    self.change_pw_status.clear();
                                }
                                Err(e) => {
                                    self.change_pw_status = format!("Failed to save: {e}");
                                }
                            }
                        }
                    } else {
                        // "old" — new password failed but old still works
                        self.change_pw_status =
                            "New password doesn't work. Old password is still active on the server.".into();
                    }
                }
                Err(msg) => {
                    self.change_pw_status = msg;
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

        // Master password prompt for Enhanced mode
        if self.master_password_prompt {
            egui::CentralPanel::default().show(ctx, |ui| {
                let palette = egui_theme::palette_from_ctx(ui.ctx());
                ui.vertical_centered(|ui| {
                    ui.add_space(60.0);
                    ui.heading("Enter Master Password");
                    ui.add_space(4.0);
                    ui.label(
                        egui::RichText::new("Your credentials are protected with a master password.")
                            .color(palette.text_secondary),
                    );
                    ui.add_space(16.0);

                    let field_width = 260.0;
                    ui.horizontal(|ui| {
                        ui.add_space((ui.available_width() - field_width - 80.0) / 2.0);
                        ui.label("Password:");
                        let resp = ui.add_sized(
                            [field_width, 22.0],
                            egui::TextEdit::singleline(&mut self.master_password_input)
                                .password(true),
                        );
                        if resp.lost_focus()
                            && ui.input(|i| i.key_pressed(egui::Key::Enter))
                        {
                            self.try_unlock_master_password();
                        }
                    });

                    ui.add_space(12.0);
                    if ui.button("Unlock").clicked() {
                        self.try_unlock_master_password();
                    }

                    if !self.master_password_error.is_empty() {
                        ui.add_space(8.0);
                        ui.colored_label(palette.error, &self.master_password_error);
                    }
                });
            });
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
                    &["Saved Entry", "Manual Entry", "Account Management"],
                )
                .show(ui);

                if self.tab_idx != before {
                    self.tab = match self.tab_idx {
                        0 => MainTab::Saved,
                        1 => MainTab::Manual,
                        _ => MainTab::Accounts,
                    };
                }

                // Push theme ComboBox to the right, aligned to top of tab bar
                ui.with_layout(egui::Layout::right_to_left(egui::Align::TOP), |ui| {
                    let themes = ["Slate", "Ember", "Fantasy", "Slate Light", "Ember Light", "Fantasy Light"];
                    let keys = ["slate", "ember", "fantasy", "slate_light", "ember_light", "fantasy_light"];
                    let current_idx = keys.iter().position(|&k| k == self.app_config.theme.as_str()).unwrap_or(0);
                    let mut selected = current_idx;
                    egui::ComboBox::from_id_salt("theme_selector")
                        .width(100.0)
                        .selected_text(themes[selected])
                        .show_ui(ui, |ui| {
                            for (i, name) in themes.iter().enumerate() {
                                if ui.selectable_value(&mut selected, i, *name).clicked() {
                                    self.app_config.theme = keys[i].to_string();
                                    self.app_config.save();
                                    self.app_config.to_theme().apply(ui.ctx());
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

        // Encryption mode change dialog (rendered as overlay Window)
        self.show_encryption_dialog(ctx);

        // Save window geometry on resize (throttle: only if changed by more than 1px)
        let viewport_info = ctx.input(|i| i.viewport().inner_rect);
        if let Some(rect) = viewport_info {
            let w = rect.width();
            let h = rect.height();
            if (w - self.app_config.window_width).abs() > 1.0
                || (h - self.app_config.window_height).abs() > 1.0
            {
                self.app_config.window_width = w;
                self.app_config.window_height = h;
                self.app_config.save();
            }
        }
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
        let mut tab_labels: Vec<String> = Vec::new();
        // FAVORITES tab always shown (even when empty)
        tab_labels.push("\u{2605} FAVORITES".to_string());
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
                // FAVORITES is always tab 0, accounts start at index 1
                let is_favorites_tab = selected_idx == 0;
                let account_idx = if selected_idx == 0 { None } else { Some(selected_idx - 1) };

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
                        ui.add_space(50.0);
                        ui.vertical_centered(|ui| {
                            ui.label("No favorite characters yet.");
                            ui.add_space(8.0);
                            ui.label("Mark characters as favorites using the \u{2605} button\nin the account tabs or saved entries list.");
                        });
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

                                let gold = egui::Color32::from_rgb(0xDA, 0xA5, 0x20);
                                let is_fantasy = self.app_config.theme == "fantasy"
                                    || self.app_config.theme == "fantasy_light";
                                let star_gold = if is_fantasy { palette.accent } else { gold };

                                let row_resp = egui::Frame::new()
                                    .fill(palette.elevated)
                                    .inner_margin(egui::Margin::symmetric(0, 2))
                                    .show(ui, |ui| {
                                        ui.horizontal(|ui| {
                                            let frontend_display = Frontend::from_str(frontend_str).display_name();
                                            let star_prefix = if self.store.accounts.iter().any(|a| {
                                                a.characters.iter().any(|c| c.name == *name && c.game_code == *game_code && c.favorite)
                                            }) { "\u{2605} " } else { "" };
                                            let play_label = format!(
                                                "{}{}  |  {}  |  {}",
                                                star_prefix, name, game_name, frontend_display
                                            );
                                            if ui
                                                .add_enabled(
                                                    !authenticating,
                                                    egui::Button::new(&play_label)
                                                        .min_size(egui::vec2(280.0, 24.0)),
                                                )
                                                .clicked()
                                            {
                                                match self.store.get_password(account, self.key.as_ref()) {
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
                                            // Remove button (red) — before star, matching lich-5 order
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
                                            // Star toggle (gold for favorite)
                                            let star_text = egui::RichText::new("\u{2605}")
                                                .color(star_gold);
                                            if ui.button(star_text).clicked() {
                                                toggle_fav = Some((
                                                    account.clone(),
                                                    name.clone(),
                                                    game_code.clone(),
                                                ));
                                            }
                                        });
                                    })
                                    .response;

                                // Paint 2px gold left border
                                let rect = row_resp.rect;
                                ui.painter().line_segment(
                                    [rect.left_top(), rect.left_bottom()],
                                    egui::Stroke::new(2.0, star_gold),
                                );
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
                                    let gold = egui::Color32::from_rgb(0xDA, 0xA5, 0x20);
                                    let is_fantasy = self.app_config.theme == "fantasy"
                                        || self.app_config.theme == "fantasy_light";
                                    let star_gold = if is_fantasy { palette.accent } else { gold };

                                    let row_resp = if *is_fav {
                                        egui::Frame::new()
                                            .fill(palette.elevated)
                                            .inner_margin(egui::Margin::symmetric(0, 2))
                                            .show(ui, |ui| {
                                                ui.horizontal(|ui| {
                                                    let frontend_display = Frontend::from_str(frontend_str).display_name();
                                                    let play_label = format!(
                                                        "\u{2605} {}  |  {}  |  {}",
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
                                                            .get_password(&account_name, self.key.as_ref())
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
                                                    // Remove button first, then star — matching lich-5 order
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
                                                    if ui.button(egui::RichText::new("\u{2605}").color(star_gold)).clicked() {
                                                        toggle_fav = Some((
                                                            account_name.clone(),
                                                            name.clone(),
                                                            game_code.clone(),
                                                        ));
                                                    }
                                                });
                                            })
                                            .response
                                    } else {
                                        ui.horizontal(|ui| {
                                            let frontend_display = Frontend::from_str(frontend_str).display_name();
                                            let star_prefix = if self.store.accounts.iter().any(|a| {
                                                a.characters.iter().any(|c| c.name == *name && c.game_code == *game_code && c.favorite)
                                            }) { "\u{2605} " } else { "" };
                                            let play_label = format!(
                                                "{}{}  |  {}  |  {}",
                                                star_prefix, name, game_name, frontend_display
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
                                                    .get_password(&account_name, self.key.as_ref())
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
                                            // Remove button first, then star — matching lich-5 order
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
                                            let star_color = palette.text_secondary;
                                            if ui.button(egui::RichText::new("\u{2606}").color(star_color)).clicked() {
                                                toggle_fav = Some((
                                                    account_name.clone(),
                                                    name.clone(),
                                                    game_code.clone(),
                                                ));
                                            }
                                        })
                                        .response
                                    };

                                    // Paint 2px gold left border for favorites
                                    if *is_fav {
                                        let rect = row_resp.rect;
                                        let painter = ui.painter();
                                        painter.line_segment(
                                            [rect.left_top(), rect.left_bottom()],
                                            egui::Stroke::new(2.0, gold),
                                        );
                                    }
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
        let right_pad = 16.0;

        // Login fields — right-aligned with padding
        let mut trigger_connect = false;
        ui.horizontal(|ui| {
            ui.add_space(ui.available_width() * 0.15); // left indent
            ui.vertical(|ui| {
                let field_width = ui.available_width() - right_pad;
                egui::Grid::new("manual_grid")
                    .num_columns(2)
                    .spacing([12.0, 6.0])
                    .show(ui, |ui| {
                        ui.label("User ID:");
                        let r = ui.add_enabled(
                            !is_fetching,
                            egui::TextEdit::singleline(&mut self.manual_account)
                                .desired_width(field_width - 80.0),
                        );
                        if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                            trigger_connect = true;
                        }
                        ui.end_row();

                        ui.label("Password:");
                        let r = ui.add_enabled(
                            !is_fetching,
                            egui::TextEdit::singleline(&mut self.manual_password)
                                .password(true)
                                .desired_width(field_width - 80.0),
                        );
                        if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                            trigger_connect = true;
                        }
                        ui.end_row();
                    });
            });
        });

        ui.add_space(6.0);

        // Disconnect (left) / Connect (right) — right-aligned with padding
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
            ui.add_space(right_pad);
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

        // Character list — fixed size with left+right padding
        let columns = vec![
            egui_theme::TreeColumn { label: "Game".into(), width: Some(180.0), sortable: true },
            egui_theme::TreeColumn { label: "Character".into(), width: None, sortable: true },
        ];

        // Character list — always show TreeView with reserved space
        {
            // Build rows from connection state
            let mut tree_rows: Vec<egui_theme::TreeRow> = match &self.connect_state {
                ConnectState::Connected(chars) => chars
                    .iter()
                    .map(|ch| egui_theme::TreeRow {
                        cells: vec![ch.game_name.clone(), ch.name.clone()],
                        children: vec![],
                        expanded: false,
                    })
                    .collect(),
                _ => vec![],
            };

            // Sort if sort column set
            if let Some(col) = self.manual_tree_sort_col {
                tree_rows.sort_by(|a, b| {
                    let cmp = a.cells.get(col).cmp(&b.cells.get(col));
                    if self.manual_tree_sort_asc { cmp } else { cmp.reverse() }
                });
            }

            // Show "Connecting..." overlay text if fetching
            if self.connect_state == ConnectState::Fetching {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label("Connecting...");
                });
            }

            // TreeView — always visible with min_body_height for empty state
            let resp = egui_theme::TreeView::new(
                "manual_chars",
                &columns,
                &mut tree_rows,
                &mut self.manual_tree_selected,
            )
            .sort_state(
                &mut self.manual_tree_sort_col,
                &mut self.manual_tree_sort_asc,
            )
            .min_body_height(120.0)
            .show(ui);

            if resp.clicked_row.is_some() {
                self.manual_selected_char = self.manual_tree_selected;
            }
        }

        ui.add_space(6.0);

        // Frontend selection — platform-gated
        ui.horizontal(|ui| {
            ui.radio_value(&mut self.manual_frontend, Frontend::Wrayth, "Wrayth");
            ui.radio_value(&mut self.manual_frontend, Frontend::Wizard, "Wizard");
            #[cfg(target_os = "macos")]
            ui.radio_value(&mut self.manual_frontend, Frontend::Avalon, "Avalon");
        });

        // Checkboxes — all visible, not conditionally hidden
        ui.checkbox(&mut self.manual_custom_launch_enabled, "Custom launch command");
        if self.manual_custom_launch_enabled {
            // Disable custom launch if Avalon selected (matching lich-5)
            #[cfg(target_os = "macos")]
            if self.manual_frontend == Frontend::Avalon {
                self.manual_custom_launch_enabled = false;
            }

            let cmd_options = launch_cmd_suggestions();
            let dir_options = launch_dir_suggestions();
            let combo_pad = right_pad + 8.0; // extra padding so dropdown doesn't touch window edge
            let combo_width = ui.available_width() - combo_pad * 2.0;
            ui.horizontal(|ui| {
                ui.add_space(combo_pad);
                ui.set_max_width(combo_width);
                egui_theme::EditableComboBox::new(
                    "manual_launch_cmd",
                    &mut self.manual_custom_launch,
                    &cmd_options,
                )
                .hint_text("(enter custom launch command)")
                .show(ui);
            });
            ui.horizontal(|ui| {
                ui.add_space(combo_pad);
                ui.set_max_width(combo_width);
                egui_theme::EditableComboBox::new(
                    "manual_launch_dir",
                    &mut self.manual_custom_launch_dir,
                    &dir_options,
                )
                .hint_text("(enter working directory for command)")
                .show(ui);
            });
        }

        ui.checkbox(&mut self.manual_save, "Save this info for quick game entry");
        ui.checkbox(&mut self.manual_favorite, "\u{2605} Mark as favorite");

        ui.add_space(6.0);

        // Play button — right-aligned with matching padding
        let can_play = self.manual_selected_char.is_some() && is_connected && !authenticating;
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
            ui.add_space(right_pad);
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
            // Legacy mode: fetch ALL characters across ALL games (like lich-5)
            let result = rt.block_on(list_all_characters(&account, &password));
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
                    let game_code = ch.game_code.clone();
                    let game_name = ch.game_name.clone();
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
                                self.store.add_account(&account, &password, self.key.as_ref())
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
                            &game_name,
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
            egui_theme::TreeColumn { label: "Account".into(), width: Some(120.0), sortable: false },
            egui_theme::TreeColumn { label: "Character".into(), width: Some(120.0), sortable: true },
            egui_theme::TreeColumn { label: "Game".into(), width: Some(120.0), sortable: true },
            egui_theme::TreeColumn { label: "Frontend".into(), width: Some(80.0), sortable: true },
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
                let is_expanded = *self.acct_tree_expanded
                    .entry(acct.account.clone())
                    .or_insert(true); // default expanded
                egui_theme::TreeRow {
                    cells: vec![
                        acct.account.to_uppercase(),
                        String::new(),
                        String::new(),
                        String::new(),
                        String::new(),
                    ],
                    children,
                    expanded: is_expanded,
                }
            })
            .collect();

        egui_theme::TreeView::new(
            "acct_tree",
            &columns,
            &mut tree_rows,
            &mut self.acct_tree_selected,
        )
        .sort_state(&mut self.acct_tree_sort_col, &mut self.acct_tree_sort_asc)
        .min_body_height(320.0)
        .show(ui);

        // Write back expanded state from tree_rows to persisted HashMap
        for (row, acct) in tree_rows.iter().zip(self.store.accounts.iter()) {
            self.acct_tree_expanded.insert(acct.account.clone(), row.expanded);
        }

        ui.add_space(8.0);

        // Resolve selected row to account/character info
        // The tree is: account rows (parent, expanded) with character children.
        // Flat index walks: acct0, child0, child1, acct1, child2, ...
        let selected_info: Option<(String, Option<(String, String)>)> = self.acct_tree_selected.and_then(|sel| {
            let mut flat_idx = 0;
            for acct in &self.store.accounts {
                if flat_idx == sel {
                    // Selected an account row
                    return Some((acct.account.clone(), None));
                }
                flat_idx += 1;
                for ch in &acct.characters {
                    if flat_idx == sel {
                        // Selected a character row
                        return Some((acct.account.clone(), Some((ch.name.clone(), ch.game_code.clone()))));
                    }
                    flat_idx += 1;
                }
            }
            None
        });

        let is_account_selected = selected_info.as_ref().map_or(false, |(_, ch)| ch.is_none());
        let is_anything_selected = selected_info.is_some();

        // Button row: Refresh, Remove, Change Password (matching lich-5)
        let mut remove_account: Option<String> = None;
        let mut remove_character: Option<(String, String, String)> = None;

        ui.horizontal(|ui| {
            if ui.button("Refresh").clicked() {
                self.store = CredentialStore::load().unwrap_or_default();
                self.accounts_status.clear();
            }

            // Remove — works on both accounts and characters (like lich-5)
            if ui
                .add_enabled(
                    is_anything_selected,
                    egui::Button::new(egui::RichText::new("Remove").color(palette.error)),
                )
                .clicked()
            {
                if let Some((acct, ch_info)) = &selected_info {
                    if let Some((char_name, game_code)) = ch_info {
                        // Remove character
                        remove_character = Some((acct.clone(), char_name.clone(), game_code.clone()));
                    } else {
                        // Remove account
                        remove_account = Some(acct.clone());
                    }
                }
            }

            // Change Password — only enabled when an account (not character) is selected
            if ui.add_enabled(is_account_selected, egui::Button::new("Change Password")).clicked() {
                if let Some((acct, _)) = &selected_info {
                    self.change_pw_account = Some(acct.clone());
                    self.change_pw_current.clear();
                    self.change_pw_new.clear();
                    self.change_pw_confirm.clear();
                    self.change_pw_status.clear();
                }
            }
        });

        // Change password dialog (matching lich-5: modal with current/new/confirm fields)
        // No server connection — purely local credential store update
        if self.change_pw_account.is_some() {
            let pw_acct = self.change_pw_account.clone().unwrap();
            let mut close_dialog = false;

            egui::Window::new("Change Password")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .default_width(400.0)
                .show(ui.ctx(), |ui| {
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        ui.label("Account:");
                        ui.label(egui::RichText::new(&pw_acct).strong());
                    });
                    ui.add_space(8.0);

                    egui::Grid::new("change_pw_grid")
                        .num_columns(2)
                        .spacing([8.0, 6.0])
                        .show(ui, |ui| {
                            ui.label("Current Password:");
                            ui.add(egui::TextEdit::singleline(&mut self.change_pw_current).password(true));
                            ui.end_row();

                            ui.label("New Password:");
                            ui.add(egui::TextEdit::singleline(&mut self.change_pw_new).password(true));
                            ui.end_row();

                            ui.label("Confirm Password:");
                            ui.add(egui::TextEdit::singleline(&mut self.change_pw_confirm).password(true));
                            ui.end_row();
                        });

                    if self.change_pw_verifying {
                        ui.add_space(4.0);
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.label(&self.change_pw_status);
                        });
                    } else if !self.change_pw_status.is_empty() {
                        ui.add_space(4.0);
                        ui.colored_label(palette.error, &self.change_pw_status);
                    }

                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                            if ui.button("Cancel").clicked() {
                                close_dialog = true;
                            }
                            if ui.add_enabled(!self.change_pw_verifying, egui::Button::new("Change Password")).clicked() {
                                // Validate
                                if self.change_pw_current.is_empty() {
                                    self.change_pw_status = "Current password cannot be empty.".into();
                                } else if self.change_pw_new.is_empty() {
                                    self.change_pw_status = "New password cannot be empty.".into();
                                } else if self.change_pw_new != self.change_pw_confirm {
                                    self.change_pw_status = "New passwords do not match.".into();
                                } else {
                                    // Verify current password against local store first
                                    match self.store.get_password(&pw_acct, self.key.as_ref()) {
                                        Ok(stored_pw) => {
                                            if stored_pw != self.change_pw_current {
                                                self.change_pw_status = "Current password is incorrect.".into();
                                            } else {
                                                // Local check passed — now verify new password works with eaccess
                                                self.change_pw_verifying = true;
                                                self.change_pw_status = "Verifying new password with server...".into();
                                                let account = pw_acct.clone();
                                                let new_pw = self.change_pw_new.clone();
                                                let old_pw = self.change_pw_current.clone();
                                                let tx = self.change_pw_tx.clone();
                                                let ctx = ui.ctx().clone();
                                                std::thread::spawn(move || {
                                                    let rt = match tokio::runtime::Runtime::new() {
                                                        Ok(rt) => rt,
                                                        Err(e) => {
                                                            let _ = tx.send(Err(format!("Runtime error: {e}")));
                                                            ctx.request_repaint();
                                                            return;
                                                        }
                                                    };
                                                    // Test new password
                                                    let new_result = rt.block_on(
                                                        list_all_characters(&account, &new_pw)
                                                    );
                                                    if new_result.is_ok() {
                                                        let _ = tx.send(Ok("new".into()));
                                                        ctx.request_repaint();
                                                        return;
                                                    }
                                                    // New password failed — test old password
                                                    let old_result = rt.block_on(
                                                        list_all_characters(&account, &old_pw)
                                                    );
                                                    if old_result.is_ok() {
                                                        // Old still works, new doesn't
                                                        let _ = tx.send(Ok("old".into()));
                                                    } else {
                                                        // Both failed
                                                        let _ = tx.send(Err(
                                                            "Neither password works with the server. Check your credentials.".into()
                                                        ));
                                                    }
                                                    ctx.request_repaint();
                                                });
                                            }
                                        }
                                        Err(e) => {
                                            self.change_pw_status = format!("Failed to verify password: {e}");
                                        }
                                    }
                                }
                            }
                        });
                    });
                });

            if close_dialog {
                self.change_pw_account = None;
                self.change_pw_current.clear();
                self.change_pw_new.clear();
                self.change_pw_confirm.clear();
                self.change_pw_status.clear();
            }
        }

        // Handle removal
        if let Some(acct) = remove_account {
            self.store.remove_account(&acct);
            let _ = self.store.save();
            self.accounts_status = format!("Removed account '{acct}'.");
            self.acct_tree_selected = None;
        }
        if let Some((acct, char_name, game_code)) = remove_character {
            self.store.remove_character(&acct, &char_name, &game_code);
            let _ = self.store.save();
            self.accounts_status = format!("Removed character '{char_name}' from '{acct}'.");
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
                            if let Err(e) = self.store.add_account(&account, &password, self.key.as_ref()) {
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
        let palette = egui_theme::palette_from_ctx(ui.ctx());

        ui.add_space(8.0);

        let mode_label = match self.enc_config.mode {
            crate::encryption::EncryptionMode::Plaintext => "Plaintext (no encryption)",
            crate::encryption::EncryptionMode::Standard => "Standard (AES-256-GCM, local key file)",
            crate::encryption::EncryptionMode::Enhanced => {
                "Enhanced (AES-256-GCM, master password + OS keychain)"
            }
        };
        ui.horizontal(|ui| {
            ui.label("Current mode:");
            ui.label(egui::RichText::new(mode_label).strong());
        });

        ui.add_space(12.0);

        if ui.button("Change Encryption Mode").clicked() {
            self.enc_new_mode = self.enc_config.mode.clone();
            self.enc_new_password.clear();
            self.enc_confirm_password.clear();
            self.enc_current_password.clear();
            self.enc_status.clear();
            self.enc_show_dialog = true;
        }

        if !self.enc_status.is_empty() && !self.enc_show_dialog {
            ui.add_space(8.0);
            let color = if self.enc_status.starts_with("Error")
                || self.enc_status.starts_with("Failed")
            {
                palette.error
            } else {
                palette.success
            };
            ui.colored_label(color, &self.enc_status);
        }
    }

    fn show_encryption_dialog(&mut self, ctx: &egui::Context) {
        use crate::encryption::EncryptionMode;

        if !self.enc_show_dialog {
            return;
        }

        let mut open = true;
        egui::Window::new("Change Encryption Mode")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .default_width(400.0)
            .show(ctx, |ui| {
                let palette = egui_theme::palette_from_ctx(ui.ctx());

                ui.add_space(4.0);

                ui.radio_value(
                    &mut self.enc_new_mode,
                    EncryptionMode::Plaintext,
                    "Plaintext \u{2014} passwords stored unencrypted",
                );
                ui.radio_value(
                    &mut self.enc_new_mode,
                    EncryptionMode::Standard,
                    "Standard \u{2014} encrypted with random key file",
                );
                ui.radio_value(
                    &mut self.enc_new_mode,
                    EncryptionMode::Enhanced,
                    "Enhanced \u{2014} encrypted with master password + OS keychain",
                );

                // If Enhanced selected: show password fields + PasswordStrengthMeter
                if self.enc_new_mode == EncryptionMode::Enhanced {
                    ui.add_space(10.0);
                    egui_theme::ThemedSeparator::fade().show(ui);
                    ui.add_space(6.0);

                    egui::Grid::new("enc_password_grid")
                        .num_columns(2)
                        .spacing([12.0, 6.0])
                        .show(ui, |ui| {
                            ui.label("New Master Password:");
                            ui.add_sized(
                                [220.0, 22.0],
                                egui::TextEdit::singleline(&mut self.enc_new_password)
                                    .password(true),
                            );
                            ui.end_row();

                            ui.label("Confirm Password:");
                            ui.add_sized(
                                [220.0, 22.0],
                                egui::TextEdit::singleline(&mut self.enc_confirm_password)
                                    .password(true),
                            );
                            ui.end_row();
                        });

                    ui.add_space(6.0);

                    egui_theme::PasswordStrengthMeter::new(&self.enc_new_password)
                        .rule("Uppercase letter", |p| p.chars().any(|c| c.is_uppercase()))
                        .rule("Lowercase letter", |p| p.chars().any(|c| c.is_lowercase()))
                        .rule("Number", |p| p.chars().any(|c| c.is_numeric()))
                        .rule("Special character", |p| {
                            p.chars().any(|c| !c.is_alphanumeric())
                        })
                        .rule("At least 12 characters", |p| p.len() >= 12)
                        .show(ui);

                    ui.add_space(4.0);

                    // Match indicator
                    if !self.enc_new_password.is_empty()
                        && !self.enc_confirm_password.is_empty()
                    {
                        if self.enc_new_password == self.enc_confirm_password {
                            ui.colored_label(palette.success, "\u{2713} Passwords match");
                        } else {
                            ui.colored_label(palette.error, "\u{2717} Passwords do not match");
                        }
                    }
                }

                // If switching FROM Enhanced: prompt current password
                if self.enc_config.mode == EncryptionMode::Enhanced
                    && self.enc_new_mode != EncryptionMode::Enhanced
                {
                    ui.add_space(10.0);
                    egui_theme::ThemedSeparator::fade().show(ui);
                    ui.add_space(6.0);
                    ui.label("Current master password (to verify):");
                    ui.add_sized(
                        [220.0, 22.0],
                        egui::TextEdit::singleline(&mut self.enc_current_password)
                            .password(true),
                    );
                }

                ui.add_space(12.0);

                // Status message
                if !self.enc_status.is_empty() {
                    let color = if self.enc_status.starts_with("Error")
                        || self.enc_status.starts_with("Failed")
                    {
                        palette.error
                    } else {
                        palette.success
                    };
                    ui.colored_label(color, &self.enc_status);
                    ui.add_space(4.0);
                }

                ui.horizontal(|ui| {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                        if ui.button("Cancel").clicked() {
                            self.enc_show_dialog = false;
                            self.enc_status.clear();
                        }
                        if ui.button("Apply").clicked() {
                            self.apply_encryption_mode_change();
                        }
                    });
                });
            });

        if !open {
            self.enc_show_dialog = false;
            self.enc_status.clear();
        }
    }

    fn try_unlock_master_password(&mut self) {
        if self.master_password_input.is_empty() {
            self.master_password_error = "Please enter your master password.".to_string();
            return;
        }

        match crate::encryption::validate_master_password(
            &self.master_password_input,
            &self.enc_config,
        ) {
            Some(key) => {
                // Store in keychain for future sessions (ignore errors)
                let _ = crate::encryption::store_key_in_keychain(&key);
                self.key = Some(key);
                self.master_password_prompt = false;
                self.master_password_input.clear();
                self.master_password_error.clear();
            }
            None => {
                self.master_password_error = "Incorrect master password.".to_string();
                self.master_password_input.clear();
            }
        }
    }

    fn apply_encryption_mode_change(&mut self) {
        use crate::encryption::EncryptionMode;

        let old_mode = &self.enc_config.mode;
        let new_mode = &self.enc_new_mode;

        // No change
        if old_mode == new_mode {
            self.enc_status = "Mode unchanged.".to_string();
            return;
        }

        // Validate inputs for Enhanced mode
        if *new_mode == EncryptionMode::Enhanced {
            if self.enc_new_password.is_empty() {
                self.enc_status = "Error: Master password is required.".to_string();
                return;
            }
            if self.enc_new_password != self.enc_confirm_password {
                self.enc_status = "Error: Passwords do not match.".to_string();
                return;
            }
            if self.enc_new_password.len() < 8 {
                self.enc_status =
                    "Error: Password must be at least 8 characters.".to_string();
                return;
            }
        }

        // If switching FROM Enhanced: verify current password
        if *old_mode == EncryptionMode::Enhanced && *new_mode != EncryptionMode::Enhanced {
            if self.enc_current_password.is_empty() {
                self.enc_status =
                    "Error: Current master password is required to switch away from Enhanced mode."
                        .to_string();
                return;
            }
            match crate::encryption::validate_master_password(
                &self.enc_current_password,
                &self.enc_config,
            ) {
                Some(_) => {} // valid — proceed
                None => {
                    self.enc_status = "Error: Incorrect current master password.".to_string();
                    return;
                }
            }
        }

        // Determine old key
        let old_key: Option<[u8; 32]> = self.key;

        // Determine new key
        let new_key: Option<[u8; 32]>;
        let mut new_config = self.enc_config.clone();
        new_config.mode = new_mode.clone();

        match new_mode {
            EncryptionMode::Plaintext => {
                new_key = None;
                new_config.test_value = None;
                new_config.salt = None;
            }
            EncryptionMode::Standard => {
                // Generate or load a random key file
                match CredentialStore::load_or_create_key() {
                    Ok(k) => new_key = Some(k),
                    Err(e) => {
                        self.enc_status = format!("Error: Failed to create key file: {e}");
                        return;
                    }
                }
                new_config.test_value = None;
                new_config.salt = None;
            }
            EncryptionMode::Enhanced => {
                use base64::Engine as _;
                let salt = crate::encryption::generate_salt();
                let derived = crate::encryption::derive_key(&self.enc_new_password, &salt);
                match crate::encryption::create_test_value(&derived) {
                    Ok(tv) => {
                        new_config.test_value = Some(tv);
                        new_config.salt = Some(
                            base64::engine::general_purpose::STANDARD.encode(&salt),
                        );
                        new_key = Some(derived);
                    }
                    Err(e) => {
                        self.enc_status =
                            format!("Error: Failed to create test value: {e}");
                        return;
                    }
                }
            }
        }

        // Re-encrypt all passwords
        if let Err(e) = crate::encryption::reencrypt_all(
            &mut self.store,
            old_key.as_ref(),
            new_key.as_ref(),
        ) {
            self.enc_status = format!("Error: Re-encryption failed: {e}");
            return;
        }

        // Save credential store
        if let Err(e) = self.store.save() {
            self.enc_status = format!("Error: Failed to save credentials: {e}");
            return;
        }

        // Save encryption config
        if let Err(e) = new_config.save() {
            self.enc_status = format!("Error: Failed to save encryption config: {e}");
            return;
        }

        // Update keychain
        match new_mode {
            EncryptionMode::Enhanced => {
                if let Some(ref k) = new_key {
                    let _ = crate::encryption::store_key_in_keychain(k);
                }
            }
            _ => {
                // Clear keychain when leaving Enhanced mode
                let _ = crate::encryption::clear_keychain();
            }
        }

        // Update self state
        self.key = new_key;
        self.enc_config = new_config;
        self.enc_show_dialog = false;
        self.enc_new_password.clear();
        self.enc_confirm_password.clear();
        self.enc_current_password.clear();
        self.enc_status = format!(
            "Encryption mode changed to {}.",
            match new_mode {
                EncryptionMode::Plaintext => "Plaintext",
                EncryptionMode::Standard => "Standard",
                EncryptionMode::Enhanced => "Enhanced",
            }
        );
    }
}
