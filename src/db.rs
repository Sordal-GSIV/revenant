use anyhow::Result;
use rusqlite::{params, Connection};

pub struct Db {
    conn: Connection,
}

impl Db {
    pub fn open(path: &str) -> Result<Self> {
        let conn = Connection::open(path)?;
        let db = Self { conn };
        db.migrate()?;
        Ok(db)
    }

    fn migrate(&self) -> Result<()> {
        self.conn.execute_batch("
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
        ")?;
        Ok(())
    }

    pub fn get_char_setting(&self, char: &str, game: &str, key: &str) -> Result<Option<String>> {
        let mut s = self.conn.prepare(
            "SELECT value FROM char_settings WHERE character=?1 AND game=?2 AND key=?3")?;
        let mut rows = s.query(params![char, game, key])?;
        Ok(rows.next()?.map(|r| r.get(0)).transpose()?)
    }

    pub fn set_char_setting(&self, char: &str, game: &str, key: &str, value: &str) -> Result<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO char_settings (character,game,key,value) VALUES(?1,?2,?3,?4)",
            params![char, game, key, value])?;
        Ok(())
    }

    pub fn delete_char_setting(&self, char: &str, game: &str, key: &str) -> Result<()> {
        self.conn.execute(
            "DELETE FROM char_settings WHERE character=?1 AND game=?2 AND key=?3",
            params![char, game, key])?;
        Ok(())
    }

    pub fn get_user_var(&self, game: &str, key: &str) -> Result<Option<String>> {
        let mut s = self.conn.prepare("SELECT value FROM user_vars WHERE game=?1 AND key=?2")?;
        let mut rows = s.query(params![game, key])?;
        Ok(rows.next()?.map(|r| r.get(0)).transpose()?)
    }

    pub fn set_user_var(&self, game: &str, key: &str, value: &str) -> Result<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO user_vars (game,key,value) VALUES(?1,?2,?3)",
            params![game, key, value])?;
        Ok(())
    }

    pub fn delete_user_var(&self, game: &str, key: &str) -> Result<()> {
        self.conn.execute(
            "DELETE FROM user_vars WHERE game=?1 AND key=?2",
            params![game, key])?;
        Ok(())
    }
}
