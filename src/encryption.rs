use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::credentials::CredentialStore;

// ---------------------------------------------------------------------------
// EncryptionMode
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EncryptionMode {
    Plaintext,
    Standard,
    Enhanced,
}

impl Default for EncryptionMode {
    fn default() -> Self {
        Self::Standard
    }
}

// ---------------------------------------------------------------------------
// EncryptionConfig
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EncryptionConfig {
    #[serde(default)]
    pub mode: EncryptionMode,
    /// Base64-encoded AES-GCM ciphertext of "revenant_test_value" (Enhanced only)
    pub test_value: Option<String>,
    /// Base64-encoded PBKDF2 salt (Enhanced only)
    pub salt: Option<String>,
}

impl Default for EncryptionConfig {
    fn default() -> Self {
        Self {
            mode: EncryptionMode::Standard,
            test_value: None,
            salt: None,
        }
    }
}

impl EncryptionConfig {
    fn config_path() -> PathBuf {
        CredentialStore::config_dir().join("encryption.toml")
    }

    pub fn load() -> Self {
        let path = Self::config_path();
        if !path.exists() {
            return Self::default();
        }
        match std::fs::read_to_string(&path) {
            Ok(contents) => toml::from_str(&contents).unwrap_or_default(),
            Err(_) => Self::default(),
        }
    }

    pub fn save(&self) -> Result<()> {
        std::fs::create_dir_all(CredentialStore::config_dir())?;
        let contents = toml::to_string_pretty(self)?;
        std::fs::write(Self::config_path(), contents)?;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Key derivation (PBKDF2-HMAC-SHA256)
// ---------------------------------------------------------------------------

pub fn derive_key(password: &str, salt: &[u8]) -> [u8; 32] {
    use pbkdf2::pbkdf2_hmac;
    use sha2::Sha256;

    let mut key = [0u8; 32];
    pbkdf2_hmac::<Sha256>(password.as_bytes(), salt, 100_000, &mut key);
    key
}

pub fn generate_salt() -> Vec<u8> {
    use aes_gcm::aead::OsRng;
    use aes_gcm::aead::rand_core::RngCore;
    let mut salt = vec![0u8; 16];
    OsRng.fill_bytes(&mut salt);
    salt
}

// ---------------------------------------------------------------------------
// Keychain helpers (keyring crate v3)
// ---------------------------------------------------------------------------

pub fn store_key_in_keychain(key: &[u8; 32]) -> Result<()> {
    use base64::Engine as _;
    let encoded = base64::engine::general_purpose::STANDARD.encode(key);
    let entry = keyring::Entry::new("revenant", "master_key")
        .map_err(|e| anyhow::anyhow!("keyring entry: {e}"))?;
    entry
        .set_password(&encoded)
        .map_err(|e| anyhow::anyhow!("keyring set: {e}"))?;
    Ok(())
}

pub fn get_key_from_keychain() -> Result<Option<[u8; 32]>> {
    use base64::Engine as _;
    let entry = match keyring::Entry::new("revenant", "master_key") {
        Ok(e) => e,
        Err(_) => return Ok(None),
    };
    match entry.get_password() {
        Ok(encoded) => {
            let bytes = base64::engine::general_purpose::STANDARD.decode(&encoded)?;
            if bytes.len() != 32 {
                return Ok(None);
            }
            let mut key = [0u8; 32];
            key.copy_from_slice(&bytes);
            Ok(Some(key))
        }
        Err(_) => Ok(None),
    }
}

pub fn clear_keychain() -> Result<()> {
    let entry = match keyring::Entry::new("revenant", "master_key") {
        Ok(e) => e,
        Err(_) => return Ok(()),
    };
    // Ignore errors — credential may not exist
    let _ = entry.delete_credential();
    Ok(())
}

// ---------------------------------------------------------------------------
// Test value (Enhanced mode validation)
// ---------------------------------------------------------------------------

const TEST_PLAINTEXT: &str = "revenant_test_value";

pub fn create_test_value(key: &[u8; 32]) -> Result<String> {
    CredentialStore::encrypt_password(TEST_PLAINTEXT, Some(key))
}

/// Validate a master password against the stored test value.
/// Returns the derived key if the password is correct, None otherwise.
pub fn validate_master_password(
    password: &str,
    config: &EncryptionConfig,
) -> Option<[u8; 32]> {
    let salt_b64 = config.salt.as_ref()?;
    let test_value = config.test_value.as_ref()?;

    use base64::Engine as _;
    let salt = base64::engine::general_purpose::STANDARD
        .decode(salt_b64)
        .ok()?;

    let key = derive_key(password, &salt);

    // Try to decrypt the test value — if it succeeds and matches, password is correct
    match CredentialStore::decrypt_password(test_value, Some(&key)) {
        Ok(plaintext) if plaintext == TEST_PLAINTEXT => Some(key),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Re-encryption
// ---------------------------------------------------------------------------

/// Re-encrypt all passwords in the credential store atomically.
/// Decrypts ALL passwords first into memory, then re-encrypts ALL.
pub fn reencrypt_all(
    store: &mut CredentialStore,
    old_key: Option<&[u8; 32]>,
    new_key: Option<&[u8; 32]>,
) -> Result<()> {
    // Phase 1: Decrypt ALL passwords into memory
    let mut plaintext_passwords: Vec<(usize, String)> = Vec::new();
    for (i, account) in store.accounts.iter().enumerate() {
        let pw = CredentialStore::decrypt_password(&account.encrypted_password, old_key)
            .map_err(|e| anyhow::anyhow!(
                "Failed to decrypt password for '{}': {e}",
                account.account
            ))?;
        plaintext_passwords.push((i, pw));
    }

    // Phase 2: Re-encrypt ALL passwords
    let mut new_encrypted: Vec<(usize, String)> = Vec::new();
    for (i, pw) in &plaintext_passwords {
        let enc = CredentialStore::encrypt_password(pw, new_key)?;
        new_encrypted.push((*i, enc));
    }

    // Phase 3: Update store (atomic — all or nothing)
    for (i, enc) in new_encrypted {
        store.accounts[i].encrypted_password = enc;
    }

    Ok(())
}
