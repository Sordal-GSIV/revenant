use revenant::eaccess::hash_password;

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
    assert_eq!(result.as_bytes()[0], 41u8);
}
