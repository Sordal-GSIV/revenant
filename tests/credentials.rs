#[cfg(feature = "login-gui")]
mod tests {
    use revenant::credentials::CredentialStore;

    #[test]
    fn test_encrypt_decrypt_roundtrip() {
        let key = [42u8; 32];
        let enc = CredentialStore::encrypt_password("MyP@ssw0rd", &key).unwrap();
        let dec = CredentialStore::decrypt_password(&enc, &key).unwrap();
        assert_eq!(dec, "MyP@ssw0rd");
    }

    #[test]
    fn test_add_and_retrieve_account() {
        let key = [7u8; 32];
        let mut store = CredentialStore::default();
        store.add_account("myaccount", "secret123", &key).unwrap();
        let pw = store.get_password("myaccount", &key).unwrap();
        assert_eq!(pw, "secret123");
    }

    #[test]
    fn test_add_character() {
        let key = [1u8; 32];
        let mut store = CredentialStore::default();
        store.add_account("acct", "pass", &key).unwrap();
        store.add_character("acct", "Aragorn", "GS3", "GemStone IV", "stormfront", None, None);
        let acct = store.accounts.iter().find(|a| a.account == "acct").unwrap();
        assert_eq!(acct.characters[0].name, "Aragorn");
    }

    #[test]
    fn test_wrong_key_fails_decrypt() {
        let key1 = [1u8; 32];
        let key2 = [2u8; 32];
        let enc = CredentialStore::encrypt_password("password", &key1).unwrap();
        assert!(CredentialStore::decrypt_password(&enc, &key2).is_err());
    }
}
