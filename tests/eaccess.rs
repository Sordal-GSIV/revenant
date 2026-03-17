use revenant::eaccess::{hash_password, parse_character_list};

#[test]
fn test_hash_password_transforms_each_byte() {
    // SGE hashing: for each byte b in password, result byte = ((b - 32) ^ key_byte) + 32
    // Key bytes loop if password is longer than key.
    let key = "ABCDE";
    let password = "hello";
    let result = hash_password(password, key);
    // Same length as password (raw bytes, not hex)
    assert_eq!(result.len(), password.len());
    // Verify first byte: 'h'=104, 'A'=65 → ((104-32) ^ 65) + 32 = (72^65)+32 = 9+32 = 41
    assert_eq!(result[0], 41u8);
}

#[test]
fn test_parse_character_list() {
    let resp = "C\t0\t2\t2\t0\tABC123\tAragorn\tDEF456\tLegolas\n";
    let entries = parse_character_list(resp, "GS3", "GemStone IV").unwrap();
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].id, "ABC123");
    assert_eq!(entries[0].name, "Aragorn");
    assert_eq!(entries[1].name, "Legolas");
}
