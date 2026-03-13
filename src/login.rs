#![cfg(feature = "monitor")]

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

pub struct LoginApp {
    // Tab selection
    tab: MainTab,

    // ── Saved Entry tab ───────────────────────────────────────────────
    store: CredentialStore,
    key: [u8; 32],
    saved_selected_account: Option<String>,
    saved_status: String,

    // ── Manual Entry tab ──────────────────────────────────────────────
    manual_account: String,
    manual_password: String,
    connect_state: ConnectState,
    manual_selected_char: Option<usize>,
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
    // Accounts sub-tab
    accounts_status: String,
    change_pw_account: Option<String>,
    change_pw_value: String,
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
    add_acct_status: String,
    add_acct_fetching: bool,
    add_acct_tx: SyncSender<Result<Vec<CharacterEntry>, String>>,
    add_acct_rx: Receiver<Result<Vec<CharacterEntry>, String>>,

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
            store,
            key,
            saved_selected_account: None,
            saved_status: String::new(),
            manual_account: String::new(),
            manual_password: String::new(),
            connect_state: ConnectState::Idle,
            manual_selected_char: None,
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
            accounts_status: String::new(),
            change_pw_account: None,
            change_pw_value: String::new(),
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
            add_acct_status: String::new(),
            add_acct_fetching: false,
            add_acct_tx,
            add_acct_rx,
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
                    let account = self.add_acct_username.clone();
                    let password = self.add_acct_password.clone();
                    if !account.is_empty() {
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
                        for ch in &chars {
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
                                    chars.len()
                                );
                            }
                            Err(e) => {
                                self.add_acct_status = format!("Failed to save: {e}");
                            }
                        }
                    }
                }
                Err(e) => {
                    self.add_acct_status = format!("Error: {e}");
                }
            }
        }

        // Auto-switch to Manual tab if no accounts
        if self.tab == MainTab::Saved && self.store.accounts.is_empty() {
            self.tab = MainTab::Manual;
        }

        // If we have a result, close the window
        if self.result.is_some() {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Revenant — Login");
            ui.add_space(8.0);

            ui.horizontal(|ui| {
                ui.selectable_value(&mut self.tab, MainTab::Saved, "Saved Entry");
                ui.selectable_value(&mut self.tab, MainTab::Manual, "Manual Entry");
                ui.selectable_value(&mut self.tab, MainTab::Accounts, "Account Management");
            });
            ui.separator();

            match self.tab.clone() {
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

        // Initialize selected account if needed
        if self.saved_selected_account.is_none() {
            let has_favorites = self
                .store
                .accounts
                .iter()
                .any(|a| a.characters.iter().any(|c| c.favorite));
            if has_favorites {
                self.saved_selected_account = Some("★ FAVORITES".to_string());
            } else if let Some(first) = self.store.accounts.first() {
                self.saved_selected_account = Some(first.account.clone());
            }
        }

        let authenticating = self.play_state == PlayState::Authenticating;
        let mut play_pending: Option<PendingPlay> = None;
        let mut toggle_fav: Option<(String, String, String)> = None;
        let mut remove_char: Option<(String, String, String)> = None;

        ui.horizontal(|ui| {
            // Left-side account selector
            ui.vertical(|ui| {
                ui.set_width(120.0);

                let has_favorites = self
                    .store
                    .accounts
                    .iter()
                    .any(|a| a.characters.iter().any(|c| c.favorite));
                if has_favorites {
                    ui.selectable_value(
                        &mut self.saved_selected_account,
                        Some("★ FAVORITES".to_string()),
                        "★ FAVORITES",
                    );
                }
                for acct in &self.store.accounts {
                    ui.selectable_value(
                        &mut self.saved_selected_account,
                        Some(acct.account.clone()),
                        acct.account.to_uppercase(),
                    );
                }
            });

            ui.separator();

            // Right side: character list for selected account
            ui.vertical(|ui| {
                let selected = self.saved_selected_account.clone();
                match selected.as_deref() {
                    Some("★ FAVORITES") => {
                        // Collect all favorite characters
                        let mut favorites: Vec<(String, String, String, String, String, Option<String>, Option<String>)> = Vec::new();
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
                                for (account, name, game_code, game_name, frontend_str, custom_launch, custom_launch_dir) in &favorites {
                                    ui.horizontal(|ui| {
                                        let frontend_display = Frontend::from_str(frontend_str).display_name();
                                        let play_label =
                                            format!("★ {}    {}    [{}]", name, game_name, frontend_display);
                                        if ui
                                            .add_enabled(
                                                !authenticating,
                                                egui::Button::new(&play_label).min_size(egui::vec2(280.0, 24.0)),
                                            )
                                            .clicked()
                                        {
                                            match self
                                                .store
                                                .get_password(account, &self.key)
                                            {
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
                                                    self.saved_status = format!(
                                                        "Failed to decrypt password: {e}"
                                                    );
                                                }
                                            }
                                        }
                                        if ui.small_button("★").clicked() {
                                            toggle_fav = Some((
                                                account.clone(),
                                                name.clone(),
                                                game_code.clone(),
                                            ));
                                        }
                                        if ui
                                            .add(
                                                egui::Button::new(
                                                    egui::RichText::new("Remove")
                                                        .color(egui::Color32::RED)
                                                        .small(),
                                                ),
                                            )
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
                    }
                    Some(account_name) => {
                        // Show characters for this account
                        let chars: Vec<(String, String, String, bool, String, Option<String>, Option<String>)> = self
                            .store
                            .accounts
                            .iter()
                            .find(|a| a.account == account_name)
                            .map(|a| {
                                a.characters
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
                                    .collect()
                            })
                            .unwrap_or_default();
                        let account_name = account_name.to_string();

                        if chars.is_empty() {
                            ui.label("No characters saved for this account.");
                        } else {
                            egui::ScrollArea::vertical().show(ui, |ui| {
                                let mut last_game: Option<String> = None;
                                for (name, game_code, game_name, is_fav, frontend_str, custom_launch, custom_launch_dir) in &chars {
                                    if last_game.as_ref() != Some(game_code) {
                                        if last_game.is_some() {
                                            ui.separator();
                                        }
                                        last_game = Some(game_code.clone());
                                    }
                                    ui.horizontal(|ui| {
                                        let frontend_display = Frontend::from_str(frontend_str).display_name();
                                        let char_display = if *is_fav {
                                            format!("★ {}    {}    [{}]", name, game_name, frontend_display)
                                        } else {
                                            format!("{}    {}    [{}]", name, game_name, frontend_display)
                                        };
                                        if ui
                                            .add_enabled(
                                                !authenticating,
                                                egui::Button::new(&char_display).min_size(egui::vec2(280.0, 24.0)),
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
                                        let star = if *is_fav { "★" } else { "☆" };
                                        if ui.small_button(star).clicked() {
                                            toggle_fav = Some((
                                                account_name.clone(),
                                                name.clone(),
                                                game_code.clone(),
                                            ));
                                        }
                                        if ui
                                            .add(
                                                egui::Button::new(
                                                    egui::RichText::new("Remove")
                                                        .color(egui::Color32::RED)
                                                        .small(),
                                                ),
                                            )
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
                    }
                    None => {
                        ui.label("Select an account.");
                    }
                }
            });
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
        if !self.saved_status.is_empty() {
            ui.colored_label(egui::Color32::RED, &self.saved_status.clone());
        }
        ui.add_space(4.0);
        if ui.button("Refresh").clicked() {
            self.store = CredentialStore::load().unwrap_or_default();
            self.saved_status.clear();
        }
    }
}

// ─── Manual Entry tab ─────────────────────────────────────────────────────────

impl LoginApp {
    fn show_manual_tab(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
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
                self.manual_status.clear();
            }
        });

        ui.add_space(6.0);

        // Character list
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
                        let chars = chars.clone();
                        for (idx, ch) in chars.iter().enumerate() {
                            let selected = self.manual_selected_char == Some(idx);
                            let label = format!("{}    {}", Self::game_name(&self.manual_game_code), ch.name);
                            if ui
                                .selectable_label(selected, &label)
                                .clicked()
                            {
                                self.manual_selected_char = Some(idx);
                            }
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
            ui.horizontal(|ui| {
                ui.label("Command:");
                ui.text_edit_singleline(&mut self.manual_custom_launch);
            });
            ui.horizontal(|ui| {
                ui.label("Directory:");
                ui.text_edit_singleline(&mut self.manual_custom_launch_dir);
            });
        }

        ui.add_space(6.0);
        ui.checkbox(&mut self.manual_save, "Save this info for quick game entry");
        if self.manual_save {
            ui.checkbox(&mut self.manual_favorite, "★ Mark as favorite");
        }

        ui.add_space(6.0);

        // Play button
        let can_play = self.manual_selected_char.is_some() && is_connected && !authenticating;
        if ui
            .add_enabled(can_play, egui::Button::new("Play"))
            .clicked()
        {
            self.manual_play();
        }

        // Status
        if !self.manual_status.is_empty() {
            ui.colored_label(egui::Color32::RED, &self.manual_status.clone());
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
        // Sub-tab buttons
        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.acct_sub_tab, AcctSubTab::Accounts, "Accounts");
            ui.selectable_value(
                &mut self.acct_sub_tab,
                AcctSubTab::AddCharacter,
                "Add Character",
            );
            ui.selectable_value(
                &mut self.acct_sub_tab,
                AcctSubTab::AddAccount,
                "Add Account",
            );
        });
        ui.separator();

        match self.acct_sub_tab.clone() {
            AcctSubTab::Accounts => self.show_acct_accounts_sub(ui),
            AcctSubTab::AddCharacter => self.show_acct_add_char_sub(ui),
            AcctSubTab::AddAccount => self.show_acct_add_account_sub(ui, ctx),
        }
    }

    fn show_acct_accounts_sub(&mut self, ui: &mut egui::Ui) {
        if self.store.accounts.is_empty() {
            ui.label("No saved accounts.");
            return;
        }

        // Snapshot for iteration
        let accounts_snapshot: Vec<_> = self
            .store
            .accounts
            .iter()
            .map(|a| {
                (
                    a.account.clone(),
                    a.characters
                        .iter()
                        .map(|c| (c.name.clone(), c.game_name.clone(), c.game_code.clone(), c.favorite))
                        .collect::<Vec<_>>(),
                )
            })
            .collect();

        let mut remove_account: Option<String> = None;
        let mut remove_char: Option<(String, String)> = None;
        let mut start_change_pw: Option<String> = None;

        egui::ScrollArea::vertical().show(ui, |ui| {
            for (acct_name, chars) in &accounts_snapshot {
                ui.horizontal(|ui| {
                    ui.strong(acct_name.to_uppercase());
                    if ui.button("Change Password").clicked() {
                        start_change_pw = Some(acct_name.clone());
                    }
                    if ui
                        .add(egui::Button::new(
                            egui::RichText::new("Remove Account").color(egui::Color32::RED),
                        ))
                        .clicked()
                    {
                        remove_account = Some(acct_name.clone());
                    }
                });

                // Inline password change
                if self.change_pw_account.as_deref() == Some(acct_name) {
                    ui.horizontal(|ui| {
                        ui.label("New password:");
                        ui.add(
                            egui::TextEdit::singleline(&mut self.change_pw_value).password(true),
                        );
                        if ui.button("Save").clicked() {
                            if let Err(e) = self.store.add_account(
                                acct_name,
                                &self.change_pw_value,
                                &self.key,
                            ) {
                                self.accounts_status = format!("Error: {e}");
                            } else {
                                // Re-add characters since add_account replaces
                                // (they are preserved via the snapshot rebuild below)
                                let _ = self.store.save();
                                self.accounts_status =
                                    format!("Password updated for '{acct_name}'.");
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

                for (name, game_name, _game_code, is_fav) in chars {
                    ui.horizontal(|ui| {
                        ui.add_space(20.0);
                        let fav_indicator = if *is_fav { " ★" } else { "" };
                        ui.label(format!("{name} ({game_name}){fav_indicator}"));
                        if ui
                            .add(egui::Button::new(
                                egui::RichText::new("Remove").color(egui::Color32::RED).small(),
                            ))
                            .clicked()
                        {
                            remove_char = Some((acct_name.clone(), name.clone()));
                        }
                    });
                }

                ui.add_space(4.0);
            }
        });

        // Handle deferred mutations
        if let Some(pw_acct) = start_change_pw {
            self.change_pw_account = Some(pw_acct);
            self.change_pw_value.clear();
        }
        if let Some(acct) = remove_account {
            self.store.remove_account(&acct);
            let _ = self.store.save();
            self.accounts_status = format!("Removed account '{acct}'.");
        }
        if let Some((acct, name)) = remove_char {
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
            self.accounts_status = format!("Removed character '{name}'.");
        }

        // Refresh + status at bottom
        ui.add_space(8.0);
        if ui.button("Refresh").clicked() {
            self.store = CredentialStore::load().unwrap_or_default();
            self.accounts_status.clear();
        }
        if !self.accounts_status.is_empty() {
            ui.colored_label(egui::Color32::GREEN, &self.accounts_status.clone());
        }
    }

    fn show_acct_add_char_sub(&mut self, ui: &mut egui::Ui) {
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
            ui.horizontal(|ui| {
                ui.label("Command:");
                ui.text_edit_singleline(&mut self.add_char_custom_launch);
            });
            ui.horizontal(|ui| {
                ui.label("Directory:");
                ui.text_edit_singleline(&mut self.add_char_custom_launch_dir);
            });
        }

        ui.add_space(6.0);

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
                self.store
                    .add_character(acct, self.add_char_name.trim(), code, name, self.add_char_frontend.as_str(), custom_launch, custom_launch_dir);
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

        if !self.add_char_status.is_empty() {
            ui.colored_label(egui::Color32::GREEN, &self.add_char_status.clone());
        }
    }

    fn show_acct_add_account_sub(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let mut trigger_add = false;

        egui::Grid::new("add_acct_grid")
            .num_columns(2)
            .spacing([12.0, 6.0])
            .show(ui, |ui| {
                ui.label("Username:");
                let r = ui.text_edit_singleline(&mut self.add_acct_username);
                if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    trigger_add = true;
                }
                ui.end_row();

                ui.label("Password:");
                let r = ui.add(
                    egui::TextEdit::singleline(&mut self.add_acct_password).password(true),
                );
                if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    trigger_add = true;
                }
                ui.end_row();
            });

        ui.add_space(6.0);

        let clicked = ui
            .add_enabled(!self.add_acct_fetching, egui::Button::new("Add Account"))
            .clicked();

        if clicked || (trigger_add && !self.add_acct_fetching) {
            if self.add_acct_username.is_empty() {
                self.add_acct_status = "Username is required.".to_string();
            } else {
                self.add_acct_fetching = true;
                self.add_acct_status = "Fetching characters...".to_string();

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
            });
        }

        if !self.add_acct_status.is_empty() {
            let color = if self.add_acct_status.starts_with("Error")
                || self.add_acct_status.starts_with("Failed")
                || self.add_acct_status.starts_with("Username")
            {
                egui::Color32::RED
            } else {
                egui::Color32::GREEN
            };
            ui.colored_label(color, &self.add_acct_status.clone());
        }
    }
}
