#![cfg(feature = "login-gui")]

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

fn default_frontend() -> String {
    "stormfront".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedCharacter {
    pub name: String,
    pub game_code: String,
    pub game_name: String,
    #[serde(default)]
    pub favorite: bool,
    #[serde(default = "default_frontend")]
    pub frontend: String,
    #[serde(default)]
    pub custom_launch: Option<String>,
    #[serde(default)]
    pub custom_launch_dir: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedAccount {
    pub account: String,
    /// AES-256-GCM encrypted password, base64-encoded: nonce(12) + ciphertext
    pub encrypted_password: String,
    pub characters: Vec<SavedCharacter>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct CredentialStore {
    pub accounts: Vec<SavedAccount>,
}

impl CredentialStore {
    pub fn config_dir() -> PathBuf {
        let mut p = dirs::config_dir().unwrap_or_else(|| PathBuf::from("."));
        p.push("revenant");
        p
    }

    pub fn accounts_path() -> PathBuf {
        Self::config_dir().join("accounts.json")
    }

    pub fn key_path() -> PathBuf {
        Self::config_dir().join("key")
    }

    pub fn load_or_create_key() -> Result<[u8; 32]> {
        let path = Self::key_path();
        std::fs::create_dir_all(Self::config_dir())?;
        if path.exists() {
            let bytes = std::fs::read(&path)?;
            if bytes.len() != 32 {
                anyhow::bail!("Invalid key file length");
            }
            let mut key = [0u8; 32];
            key.copy_from_slice(&bytes);
            Ok(key)
        } else {
            use std::io::Write;
            let key = rand_key();
            std::fs::File::create(&path)?.write_all(&key)?;
            Ok(key)
        }
    }

    pub fn load() -> Result<Self> {
        let path = Self::accounts_path();
        if !path.exists() {
            return Ok(Self::default());
        }
        let json = std::fs::read_to_string(&path)?;
        Ok(serde_json::from_str(&json)?)
    }

    pub fn save(&self) -> Result<()> {
        std::fs::create_dir_all(Self::config_dir())?;
        std::fs::write(Self::accounts_path(), serde_json::to_string_pretty(self)?)?;
        Ok(())
    }

    pub fn encrypt_password(password: &str, key: &[u8; 32]) -> Result<String> {
        use aes_gcm::{
            aead::{Aead, AeadCore, OsRng},
            Aes256Gcm, KeyInit,
        };
        use base64::Engine as _;
        let cipher = Aes256Gcm::new(key.into());
        let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
        let ciphertext = cipher
            .encrypt(&nonce, password.as_bytes())
            .map_err(|e| anyhow::anyhow!("encrypt: {e}"))?;
        let mut combined = nonce.to_vec();
        combined.extend_from_slice(&ciphertext);
        Ok(base64::engine::general_purpose::STANDARD.encode(&combined))
    }

    pub fn decrypt_password(encrypted: &str, key: &[u8; 32]) -> Result<String> {
        use aes_gcm::{aead::Aead, Aes256Gcm, KeyInit, Nonce};
        use base64::Engine as _;
        let bytes = base64::engine::general_purpose::STANDARD.decode(encrypted)?;
        if bytes.len() < 12 {
            anyhow::bail!("Invalid encrypted password");
        }
        let (nonce_bytes, ct) = bytes.split_at(12);
        let cipher = Aes256Gcm::new(key.into());
        let nonce = Nonce::from_slice(nonce_bytes);
        let plaintext = cipher
            .decrypt(nonce, ct)
            .map_err(|e| anyhow::anyhow!("decrypt: {e}"))?;
        Ok(String::from_utf8(plaintext)?)
    }

    pub fn add_account(&mut self, account: &str, password: &str, key: &[u8; 32]) -> Result<()> {
        let enc = Self::encrypt_password(password, key)?;
        self.accounts
            .retain(|a| a.account.to_lowercase() != account.to_lowercase());
        self.accounts.push(SavedAccount {
            account: account.to_string(),
            encrypted_password: enc,
            characters: vec![],
        });
        Ok(())
    }

    pub fn add_character(
        &mut self,
        account: &str,
        name: &str,
        game_code: &str,
        game_name: &str,
        frontend: &str,
        custom_launch: Option<String>,
        custom_launch_dir: Option<String>,
    ) {
        if let Some(a) = self
            .accounts
            .iter_mut()
            .find(|a| a.account.to_lowercase() == account.to_lowercase())
        {
            // Preserve favorite status if the character already existed
            let was_favorite = a.characters.iter()
                .find(|c| c.name.to_lowercase() == name.to_lowercase())
                .map(|c| c.favorite)
                .unwrap_or(false);
            a.characters
                .retain(|c| c.name.to_lowercase() != name.to_lowercase());
            a.characters.push(SavedCharacter {
                name: name.to_string(),
                game_code: game_code.to_string(),
                game_name: game_name.to_string(),
                favorite: was_favorite,
                frontend: frontend.to_string(),
                custom_launch,
                custom_launch_dir,
            });
        }
    }

    pub fn toggle_favorite(
        &mut self,
        account: &str,
        char_name: &str,
        game_code: &str,
    ) -> bool {
        if let Some(a) = self
            .accounts
            .iter_mut()
            .find(|a| a.account.to_lowercase() == account.to_lowercase())
        {
            if let Some(ch) = a.characters.iter_mut().find(|c| {
                c.name.to_lowercase() == char_name.to_lowercase()
                    && c.game_code == game_code
            }) {
                ch.favorite = !ch.favorite;
                return ch.favorite;
            }
        }
        false
    }

    pub fn remove_account(&mut self, account: &str) {
        self.accounts
            .retain(|a| a.account.to_lowercase() != account.to_lowercase());
    }

    pub fn get_password(&self, account: &str, key: &[u8; 32]) -> Result<String> {
        let entry = self
            .accounts
            .iter()
            .find(|a| a.account.to_lowercase() == account.to_lowercase())
            .ok_or_else(|| anyhow::anyhow!("Account not found: {account}"))?;
        Self::decrypt_password(&entry.encrypted_password, key)
    }
}

fn rand_key() -> [u8; 32] {
    use aes_gcm::aead::OsRng;
    use aes_gcm::aead::rand_core::RngCore;
    let mut key = [0u8; 32];
    OsRng.fill_bytes(&mut key);
    key
}
