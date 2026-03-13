#![cfg(feature = "monitor")]

use crate::credentials::{CredentialStore, SavedCharacter};
use crate::eaccess::{list_characters, CharacterEntry};
use eframe::egui;
use std::sync::mpsc::{sync_channel, Receiver, SyncSender};

/// The result returned when the user clicks Play.
#[derive(Debug, Clone)]
pub struct LoginResult {
    pub account: String,
    pub password: String,
    pub game_code: String,
    pub character: String,
}

#[derive(Debug, Clone, PartialEq)]
enum Tab {
    Saved,
    Manual,
    Accounts,
}

#[derive(Debug, Clone, PartialEq)]
enum FetchOrigin {
    Manual,
    Accounts,
}

/// Status of a background character-fetch operation.
enum FetchState {
    Idle,
    Fetching,
    Done(Vec<CharacterEntry>),
    Error(String),
}

pub struct LoginApp {
    // Which tab is active
    tab: Tab,

    // ── Manual tab ──────────────────────────────────────────────────────────
    manual_account: String,
    manual_password: String,
    manual_game: String,
    manual_character: String,
    manual_characters: Vec<CharacterEntry>,
    fetch_state: FetchState,
    fetch_tx: SyncSender<(FetchOrigin, Result<Vec<CharacterEntry>, String>)>,
    fetch_rx: Receiver<(FetchOrigin, Result<Vec<CharacterEntry>, String>)>,

    // ── Accounts tab ────────────────────────────────────────────────────────
    acct_tab_account: String,
    acct_tab_password: String,
    acct_tab_game: String,
    acct_tab_status: String,

    // ── Shared credential store ──────────────────────────────────────────
    store: CredentialStore,
    key: [u8; 32],

    // ── Result ──────────────────────────────────────────────────────────────
    pub result: Option<LoginResult>,

    // Error message shown to user
    error: String,
}

impl LoginApp {
    pub fn new() -> Self {
        let (fetch_tx, fetch_rx) = sync_channel(1);
        let key = CredentialStore::load_or_create_key().unwrap_or([0u8; 32]);
        let store = CredentialStore::load().unwrap_or_default();

        Self {
            tab: Tab::Saved,
            manual_account: String::new(),
            manual_password: String::new(),
            manual_game: "GS3".to_string(),
            manual_character: String::new(),
            manual_characters: vec![],
            fetch_state: FetchState::Idle,
            fetch_tx,
            fetch_rx,
            acct_tab_account: String::new(),
            acct_tab_password: String::new(),
            acct_tab_game: "GS3".to_string(),
            acct_tab_status: String::new(),
            store,
            key,
            result: None,
            error: String::new(),
        }
    }

    fn game_options() -> Vec<(&'static str, &'static str)> {
        vec![
            ("GS3", "GemStone IV"),
            ("DR", "DragonRealms"),
            ("GSF", "GemStone IV Prime F2P"),
        ]
    }

    fn game_name(code: &str) -> &'static str {
        for (c, n) in Self::game_options() {
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

impl eframe::App for LoginApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Poll background fetch
        if let Ok((origin, res)) = self.fetch_rx.try_recv() {
            self.fetch_state = FetchState::Idle;
            match (origin, res) {
                (FetchOrigin::Manual, Ok(chars)) => {
                    self.manual_characters = chars;
                    if !self.manual_characters.is_empty() {
                        self.manual_character =
                            self.manual_characters[0].name.clone();
                    }
                }
                (FetchOrigin::Manual, Err(e)) => {
                    self.fetch_state = FetchState::Error(e);
                }
                (FetchOrigin::Accounts, Ok(chars)) => {
                    self.fetch_state = FetchState::Done(chars);
                }
                (FetchOrigin::Accounts, Err(e)) => {
                    self.acct_tab_status = format!("Error: {e}");
                }
            }
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
                ui.selectable_value(&mut self.tab, Tab::Saved, "Saved Characters");
                ui.selectable_value(&mut self.tab, Tab::Manual, "Manual");
                ui.selectable_value(&mut self.tab, Tab::Accounts, "Manage Accounts");
            });
            ui.separator();

            match self.tab.clone() {
                Tab::Saved => self.show_saved_tab(ui),
                Tab::Manual => self.show_manual_tab(ui),
                Tab::Accounts => self.show_accounts_tab(ui),
            }

            if !self.error.is_empty() {
                ui.add_space(8.0);
                ui.colored_label(egui::Color32::RED, &self.error.clone());
            }
        });
    }
}

impl LoginApp {
    fn show_saved_tab(&mut self, ui: &mut egui::Ui) {
        if self.store.accounts.is_empty() {
            ui.label("No saved accounts. Use the Manage Accounts tab to add one.");
            return;
        }

        let mut play_result: Option<LoginResult> = None;

        egui::ScrollArea::vertical().show(ui, |ui| {
            // Clone accounts list to avoid borrow issues
            let accounts: Vec<_> = self.store.accounts.iter().map(|a| {
                (a.account.clone(), a.encrypted_password.clone(), a.characters.clone())
            }).collect();

            for (account_name, _enc_pw, characters) in &accounts {
                let id = egui::Id::new(format!("acct_{account_name}"));
                egui::CollapsingHeader::new(account_name)
                    .id_salt(id)
                    .default_open(true)
                    .show(ui, |ui| {
                        if characters.is_empty() {
                            ui.label("  No characters saved for this account.");
                            ui.label("  Use Manage Accounts tab to fetch characters.");
                        } else {
                            for ch in characters {
                                ui.horizontal(|ui| {
                                    ui.label(format!("  {} ({})", ch.name, ch.game_name));
                                    if ui.button("▶ Play").clicked() {
                                        // Decrypt password
                                        match self.store.get_password(account_name, &self.key) {
                                            Ok(pw) => {
                                                play_result = Some(LoginResult {
                                                    account: account_name.clone(),
                                                    password: pw,
                                                    game_code: ch.game_code.clone(),
                                                    character: ch.name.clone(),
                                                });
                                            }
                                            Err(e) => {
                                                self.error = format!("Failed to decrypt password: {e}");
                                            }
                                        }
                                    }
                                });
                            }
                        }
                    });
            }
        });

        if let Some(r) = play_result {
            self.result = Some(r);
        }
    }

    fn show_manual_tab(&mut self, ui: &mut egui::Ui) {
        egui::Grid::new("manual_grid")
            .num_columns(2)
            .spacing([12.0, 6.0])
            .show(ui, |ui| {
                ui.label("Account:");
                ui.text_edit_singleline(&mut self.manual_account);
                ui.end_row();

                ui.label("Password:");
                ui.add(egui::TextEdit::singleline(&mut self.manual_password).password(true));
                ui.end_row();

                ui.label("Game:");
                egui::ComboBox::from_id_salt("manual_game")
                    .selected_text(Self::game_name(&self.manual_game))
                    .show_ui(ui, |ui| {
                        for (code, name) in Self::game_options() {
                            ui.selectable_value(&mut self.manual_game, code.to_string(), name);
                        }
                    });
                ui.end_row();
            });

        ui.add_space(6.0);

        let fetching = matches!(self.fetch_state, FetchState::Fetching);
        if ui
            .add_enabled(!fetching, egui::Button::new("Fetch Characters"))
            .clicked()
        {
            let account = self.manual_account.clone();
            let password = self.manual_password.clone();
            let game = self.manual_game.clone();
            let tx = self.fetch_tx.clone();
            self.fetch_state = FetchState::Fetching;
            self.error.clear();

            // Spawn on a new tokio runtime thread
            std::thread::spawn(move || {
                let rt = match tokio::runtime::Runtime::new() {
                    Ok(rt) => rt,
                    Err(e) => {
                        let _ = tx.send((FetchOrigin::Manual, Err(e.to_string())));
                        return;
                    }
                };
                let result = rt.block_on(list_characters(&account, &password, &game));
                let _ = tx.send((FetchOrigin::Manual, result.map_err(|e| format!("{e:#}"))));
            });
        }

        match &self.fetch_state {
            FetchState::Fetching => {
                ui.spinner();
                ui.label("Fetching characters...");
            }
            FetchState::Error(e) => {
                ui.colored_label(egui::Color32::RED, format!("Fetch error: {e}"));
            }
            _ => {}
        }

        if !self.manual_characters.is_empty() {
            ui.add_space(6.0);
            ui.label("Character:");
            egui::ComboBox::from_id_salt("manual_character")
                .selected_text(&self.manual_character)
                .show_ui(ui, |ui| {
                    let chars: Vec<String> =
                        self.manual_characters.iter().map(|c| c.name.clone()).collect();
                    for name in chars {
                        ui.selectable_value(&mut self.manual_character, name.clone(), &name);
                    }
                });
        } else {
            ui.add_space(6.0);
            ui.label("Character:");
            ui.text_edit_singleline(&mut self.manual_character);
        }

        ui.add_space(10.0);

        if ui.button("▶ Play").clicked() {
            if self.manual_account.is_empty() {
                self.error = "Account is required.".to_string();
            } else if self.manual_character.is_empty() {
                self.error = "Character is required.".to_string();
            } else {
                self.result = Some(LoginResult {
                    account: self.manual_account.clone(),
                    password: self.manual_password.clone(),
                    game_code: self.manual_game.clone(),
                    character: self.manual_character.clone(),
                });
            }
        }
    }

    fn show_accounts_tab(&mut self, ui: &mut egui::Ui) {
        ui.label("Add or update a saved account:");
        ui.add_space(4.0);

        egui::Grid::new("accounts_grid")
            .num_columns(2)
            .spacing([12.0, 6.0])
            .show(ui, |ui| {
                ui.label("Account:");
                ui.text_edit_singleline(&mut self.acct_tab_account);
                ui.end_row();

                ui.label("Password:");
                ui.add(egui::TextEdit::singleline(&mut self.acct_tab_password).password(true));
                ui.end_row();

                ui.label("Game:");
                egui::ComboBox::from_id_salt("acct_game")
                    .selected_text(Self::game_name(&self.acct_tab_game))
                    .show_ui(ui, |ui| {
                        for (code, name) in Self::game_options() {
                            ui.selectable_value(&mut self.acct_tab_game, code.to_string(), name);
                        }
                    });
                ui.end_row();
            });

        ui.add_space(6.0);

        ui.horizontal(|ui| {
            if ui.button("Save Account").clicked() {
                self.acct_tab_status.clear();
                self.error.clear();
                if self.acct_tab_account.is_empty() {
                    self.error = "Account name is required.".to_string();
                } else {
                    match self.store.add_account(
                        &self.acct_tab_account.clone(),
                        &self.acct_tab_password.clone(),
                        &self.key,
                    ) {
                        Ok(()) => {
                            match self.store.save() {
                                Ok(()) => {
                                    self.acct_tab_status =
                                        format!("Saved account '{}'.", self.acct_tab_account);
                                }
                                Err(e) => {
                                    self.error = format!("Failed to save: {e}");
                                }
                            }
                        }
                        Err(e) => {
                            self.error = format!("Failed to encrypt: {e}");
                        }
                    }
                }
            }

            if ui.button("Fetch & Save Characters").clicked() {
                self.acct_tab_status.clear();
                self.error.clear();
                let account = self.acct_tab_account.clone();
                let password = self.acct_tab_password.clone();
                let game = self.acct_tab_game.clone();
                let tx = self.fetch_tx.clone();
                self.fetch_state = FetchState::Fetching;

                std::thread::spawn(move || {
                    let rt = match tokio::runtime::Runtime::new() {
                        Ok(rt) => rt,
                        Err(e) => {
                            let _ = tx.send((FetchOrigin::Accounts, Err(e.to_string())));
                            return;
                        }
                    };
                    let result = rt.block_on(list_characters(&account, &password, &game));
                    let _ = tx.send((FetchOrigin::Accounts, result.map_err(|e| format!("{e:#}"))));
                });
            }
        });

        if !self.acct_tab_status.is_empty() {
            ui.colored_label(egui::Color32::GREEN, &self.acct_tab_status.clone());
        }

        // Handle fetch result in accounts tab context
        if let FetchState::Done(ref chars) = &self.fetch_state {
            let chars = chars.clone();
            let account = self.acct_tab_account.clone();
            let game_code = self.acct_tab_game.clone();
            let game_name = Self::game_name(&game_code).to_string();

            // Add account if not already saved
            if !account.is_empty() {
                // Ensure account exists in store
                let exists = self
                    .store
                    .accounts
                    .iter()
                    .any(|a| a.account.to_lowercase() == account.to_lowercase());
                if !exists {
                    if let Err(e) =
                        self.store
                            .add_account(&account, &self.acct_tab_password, &self.key)
                    {
                        self.error = format!("Failed to save account: {e}");
                    }
                }
                for ch in &chars {
                    self.store
                        .add_character(&account, &ch.name, &game_code, &game_name);
                }
                match self.store.save() {
                    Ok(()) => {
                        self.acct_tab_status = format!(
                            "Saved {} character(s) for '{account}'.",
                            chars.len()
                        );
                    }
                    Err(e) => {
                        self.error = format!("Failed to save: {e}");
                    }
                }
            }
            self.fetch_state = FetchState::Idle;
        }

        ui.add_space(12.0);
        ui.separator();
        ui.heading("Saved Accounts");

        let accounts_snapshot: Vec<String> =
            self.store.accounts.iter().map(|a| a.account.clone()).collect();

        let mut to_remove: Option<String> = None;

        for acct_name in &accounts_snapshot {
            ui.horizontal(|ui| {
                let char_count = self
                    .store
                    .accounts
                    .iter()
                    .find(|a| &a.account == acct_name)
                    .map(|a| a.characters.len())
                    .unwrap_or(0);
                ui.label(format!("{acct_name} ({char_count} characters)"));
                if ui.button("Remove").clicked() {
                    to_remove = Some(acct_name.clone());
                }
            });
        }

        if let Some(name) = to_remove {
            self.store.remove_account(&name);
            let _ = self.store.save();
        }
    }
}
