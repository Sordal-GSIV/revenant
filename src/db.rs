use anyhow::Result;
use rusqlite::{params, Connection};
use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub struct Db {
    conn: Arc<Mutex<Connection>>,
}

impl Db {
    pub fn open(path: &str) -> Result<Self> {
        let conn = Connection::open(path)?;
        let db = Self { conn: Arc::new(Mutex::new(conn)) };
        db.migrate()?;
        Ok(db)
    }

    fn migrate(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute_batch("
            CREATE TABLE IF NOT EXISTS char_settings (
                character TEXT NOT NULL,
                game      TEXT NOT NULL,
                key       TEXT NOT NULL,
                value     TEXT NOT NULL,
                PRIMARY KEY (character, game, key)
            );
            CREATE TABLE IF NOT EXISTS user_vars (
                game  TEXT NOT NULL,
                key   TEXT NOT NULL,
                value TEXT NOT NULL,
                PRIMARY KEY (game, key)
            );
            CREATE TABLE IF NOT EXISTS map_rooms (
                id          INTEGER PRIMARY KEY,
                game        TEXT NOT NULL,
                name        TEXT NOT NULL,
                description TEXT,
                waypoint    TEXT
            );
            CREATE TABLE IF NOT EXISTS map_exits (
                room_id   INTEGER NOT NULL,
                direction TEXT NOT NULL,
                dest_id   INTEGER NOT NULL,
                UNIQUE (room_id, direction)
            );
            CREATE INDEX IF NOT EXISTS idx_map_exits_room ON map_exits(room_id);
            CREATE TABLE IF NOT EXISTS char_data (
                character TEXT NOT NULL,
                game      TEXT NOT NULL,
                key       TEXT NOT NULL,
                value     TEXT NOT NULL,
                PRIMARY KEY (character, game, key)
            );
        ")?;
        Ok(())
    }

    pub fn get_char_setting(&self, char: &str, game: &str, key: &str) -> Result<Option<String>> {
        let conn = self.conn.lock().unwrap();
        let mut s = conn.prepare(
            "SELECT value FROM char_settings WHERE character=?1 AND game=?2 AND key=?3")?;
        let mut rows = s.query(params![char, game, key])?;
        Ok(rows.next()?.map(|r| r.get(0)).transpose()?)
    }

    pub fn set_char_setting(&self, char: &str, game: &str, key: &str, value: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT OR REPLACE INTO char_settings (character,game,key,value) VALUES(?1,?2,?3,?4)",
            params![char, game, key, value])?;
        Ok(())
    }

    pub fn delete_char_setting(&self, char: &str, game: &str, key: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "DELETE FROM char_settings WHERE character=?1 AND game=?2 AND key=?3",
            params![char, game, key])?;
        Ok(())
    }

    pub fn list_char_settings(&self, char: &str, game: &str, prefix: &str) -> Result<Vec<(String, String)>> {
        let conn = self.conn.lock().unwrap();
        let pattern = format!("{}%", prefix);
        let mut s = conn.prepare(
            "SELECT key, value FROM char_settings WHERE character=?1 AND game=?2 AND key LIKE ?3"
        )?;
        let rows = s.query_map(params![char, game, pattern], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;
        Ok(rows.filter_map(|r| r.ok()).collect())
    }

    pub fn get_user_var(&self, game: &str, key: &str) -> Result<Option<String>> {
        let conn = self.conn.lock().unwrap();
        let mut s = conn.prepare("SELECT value FROM user_vars WHERE game=?1 AND key=?2")?;
        let mut rows = s.query(params![game, key])?;
        Ok(rows.next()?.map(|r| r.get(0)).transpose()?)
    }

    pub fn set_user_var(&self, game: &str, key: &str, value: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT OR REPLACE INTO user_vars (game,key,value) VALUES(?1,?2,?3)",
            params![game, key, value])?;
        Ok(())
    }

    pub fn delete_user_var(&self, game: &str, key: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "DELETE FROM user_vars WHERE game=?1 AND key=?2",
            params![game, key])?;
        Ok(())
    }

    pub fn list_user_vars(&self, game: &str) -> Result<Vec<(String, String)>> {
        let conn = self.conn.lock().unwrap();
        let mut s = conn.prepare("SELECT key, value FROM user_vars WHERE game=?1")?;
        let rows = s.query_map(params![game], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;
        Ok(rows.filter_map(|r| r.ok()).collect())
    }

    pub fn get_char_data(&self, char: &str, game: &str, key: &str) -> Result<Option<String>> {
        let conn = self.conn.lock().unwrap();
        let mut s = conn.prepare(
            "SELECT value FROM char_data WHERE character=?1 AND game=?2 AND key=?3")?;
        let mut rows = s.query(params![char, game, key])?;
        Ok(rows.next()?.map(|r| r.get(0)).transpose()?)
    }

    pub fn set_char_data(&self, char: &str, game: &str, key: &str, value: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT OR REPLACE INTO char_data (character,game,key,value) VALUES(?1,?2,?3,?4)",
            params![char, game, key, value])?;
        Ok(())
    }

    pub fn set_char_data_batch(&self, char: &str, game: &str, pairs: &[(&str, &str)]) -> Result<()> {
        if pairs.is_empty() { return Ok(()); }
        let conn = self.conn.lock().unwrap();
        let tx = conn.unchecked_transaction()?;
        {
            let mut stmt = tx.prepare(
                "INSERT OR REPLACE INTO char_data (character,game,key,value) VALUES(?1,?2,?3,?4)")?;
            for (key, value) in pairs {
                stmt.execute(params![char, game, key, value])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    pub fn get_char_data_prefix(&self, char: &str, game: &str, prefix: &str) -> Result<Vec<(String, String)>> {
        let escaped = prefix.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_");
        let pattern = format!("{escaped}%");
        let conn = self.conn.lock().unwrap();
        let mut s = conn.prepare(
            "SELECT key, value FROM char_data WHERE character=?1 AND game=?2 AND key LIKE ?3 ESCAPE '\\'")?;
        let rows = s.query_map(params![char, game, pattern], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;
        let mut result = Vec::new();
        for row in rows {
            result.push(row?);
        }
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_db() -> Db {
        Db::open(":memory:").unwrap()
    }

    #[test]
    fn test_char_data_crud() {
        let db = test_db();
        assert_eq!(db.get_char_data("Ondreian", "GS3", "stat.race").unwrap(), None);

        db.set_char_data("Ondreian", "GS3", "stat.race", "Human").unwrap();
        assert_eq!(db.get_char_data("Ondreian", "GS3", "stat.race").unwrap(), Some("Human".to_string()));

        db.set_char_data("Ondreian", "GS3", "stat.race", "Half-Elf").unwrap();
        assert_eq!(db.get_char_data("Ondreian", "GS3", "stat.race").unwrap(), Some("Half-Elf".to_string()));
    }

    #[test]
    fn test_char_data_batch() {
        let db = test_db();
        let pairs = vec![
            ("stat.strength", "87"),
            ("stat.strength_bonus", "12"),
            ("stat.constitution", "80"),
        ];
        db.set_char_data_batch("Ondreian", "GS3", &pairs).unwrap();

        assert_eq!(db.get_char_data("Ondreian", "GS3", "stat.strength").unwrap(), Some("87".to_string()));
        assert_eq!(db.get_char_data("Ondreian", "GS3", "stat.constitution").unwrap(), Some("80".to_string()));
    }

    #[test]
    fn test_char_data_prefix() {
        let db = test_db();
        db.set_char_data("Ondreian", "GS3", "stat.strength", "87").unwrap();
        db.set_char_data("Ondreian", "GS3", "stat.constitution", "80").unwrap();
        db.set_char_data("Ondreian", "GS3", "skill.edged_weapons", "30").unwrap();

        let stats = db.get_char_data_prefix("Ondreian", "GS3", "stat.").unwrap();
        assert_eq!(stats.len(), 2);

        let skills = db.get_char_data_prefix("Ondreian", "GS3", "skill.").unwrap();
        assert_eq!(skills.len(), 1);
    }
}
