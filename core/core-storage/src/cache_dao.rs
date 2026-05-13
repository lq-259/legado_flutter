use rusqlite::{Connection, Result as SqlResult, params};
use chrono::Utc;

pub struct CacheDao<'a> {
    conn: &'a Connection,
}

impl<'a> CacheDao<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    pub fn get(&self, key: &str) -> SqlResult<Option<String>> {
        let mut stmt = self.conn.prepare(
            "SELECT value FROM legacy_cache WHERE key = ?"
        )?;
        let mut rows = stmt.query(params![key])?;
        Ok(rows.next()?.map(|row| row.get(0).unwrap_or_default()))
    }

    pub fn put(&self, key: &str, value: &str) -> SqlResult<()> {
        let now = Utc::now().timestamp();
        self.conn.execute(
            "INSERT INTO legacy_cache (key, value, updated_at) VALUES (?1, ?2, ?3)
             ON CONFLICT(key) DO UPDATE SET value = ?2, updated_at = ?3",
            params![key, value, now],
        )?;
        Ok(())
    }

    pub fn delete(&self, key: &str) -> SqlResult<()> {
        self.conn.execute("DELETE FROM legacy_cache WHERE key = ?", params![key])?;
        Ok(())
    }
}
