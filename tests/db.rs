use revenant::db::Db;

#[test]
fn test_db_open_creates_tables() {
    let db = Db::open(":memory:").unwrap();
    assert!(db.get_char_setting("C", "GS3", "x").unwrap().is_none());
}

#[test]
fn test_char_setting_roundtrip() {
    let db = Db::open(":memory:").unwrap();
    db.set_char_setting("C", "GS3", "autoloot", "true").unwrap();
    assert_eq!(db.get_char_setting("C", "GS3", "autoloot").unwrap(), Some("true".into()));
}

#[test]
fn test_char_setting_delete() {
    let db = Db::open(":memory:").unwrap();
    db.set_char_setting("C", "GS3", "k", "v").unwrap();
    db.delete_char_setting("C", "GS3", "k").unwrap();
    assert!(db.get_char_setting("C", "GS3", "k").unwrap().is_none());
}

#[test]
fn test_user_var_roundtrip() {
    let db = Db::open(":memory:").unwrap();
    db.set_user_var("GS3", "threshold", "500").unwrap();
    assert_eq!(db.get_user_var("GS3", "threshold").unwrap(), Some("500".into()));
}
