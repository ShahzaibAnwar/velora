//! SQLite persistence for download records and settings.

use std::path::Path;
use std::sync::Mutex;

use rusqlite::{params, Connection, OptionalExtension};

use crate::model::{DownloadRecord, ErrorInfo};
use crate::settings::Settings;

const MIGRATIONS: &[&str] = &[include_str!("../migrations/0001_init.sql")];

pub struct Store {
    conn: Mutex<Connection>,
}

fn db_error(e: rusqlite::Error) -> ErrorInfo {
    ErrorInfo::new("storage", format!("Velora's database reported an error: {e}"))
}

impl Store {
    pub fn open(path: &Path) -> Result<Self, ErrorInfo> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| ErrorInfo::new("storage", format!("Couldn't create the data folder: {e}")))?;
        }
        let conn = Connection::open(path).map_err(db_error)?;
        Self::init(conn)
    }

    pub fn open_in_memory() -> Result<Self, ErrorInfo> {
        Self::init(Connection::open_in_memory().map_err(db_error)?)
    }

    fn init(conn: Connection) -> Result<Self, ErrorInfo> {
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL; PRAGMA foreign_keys=ON; PRAGMA busy_timeout=5000;")
            .map_err(db_error)?;
        let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0)).map_err(db_error)?;
        for (i, sql) in MIGRATIONS.iter().enumerate().skip(version as usize) {
            conn.execute_batch(&format!("BEGIN; {sql}; PRAGMA user_version = {}; COMMIT;", i + 1)).map_err(db_error)?;
        }
        Ok(Self { conn: Mutex::new(conn) })
    }

    pub fn load_downloads(&self) -> Result<Vec<DownloadRecord>, ErrorInfo> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT id, data FROM downloads ORDER BY created_at").map_err(db_error)?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))).map_err(db_error)?;
        let mut out = Vec::new();
        for row in rows {
            let (id, data) = row.map_err(db_error)?;
            match serde_json::from_str::<DownloadRecord>(&data) {
                Ok(rec) => out.push(rec),
                Err(e) => tracing::warn!(%id, error = %e, "skipping unreadable download record"),
            }
        }
        Ok(out)
    }

    pub fn upsert(&self, rec: &DownloadRecord) -> Result<(), ErrorInfo> {
        let data = serde_json::to_string(rec).map_err(|e| ErrorInfo::new("storage", e.to_string()))?;
        self.conn
            .lock()
            .unwrap()
            .execute(
                "INSERT INTO downloads (id, status, created_at, completed_at, data) VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT(id) DO UPDATE SET status = excluded.status, completed_at = excluded.completed_at, data = excluded.data",
                params![rec.id, rec.status.as_str(), rec.created_at, rec.completed_at, data],
            )
            .map_err(db_error)?;
        Ok(())
    }

    pub fn upsert_many(&self, recs: &[DownloadRecord]) -> Result<(), ErrorInfo> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction().map_err(db_error)?;
        for rec in recs {
            let data = serde_json::to_string(rec).map_err(|e| ErrorInfo::new("storage", e.to_string()))?;
            tx.execute(
                "INSERT INTO downloads (id, status, created_at, completed_at, data) VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT(id) DO UPDATE SET status = excluded.status, completed_at = excluded.completed_at, data = excluded.data",
                params![rec.id, rec.status.as_str(), rec.created_at, rec.completed_at, data],
            )
            .map_err(db_error)?;
        }
        tx.commit().map_err(db_error)
    }

    pub fn delete(&self, ids: &[String]) -> Result<(), ErrorInfo> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction().map_err(db_error)?;
        for id in ids {
            tx.execute("DELETE FROM downloads WHERE id = ?1", params![id]).map_err(db_error)?;
        }
        tx.commit().map_err(db_error)
    }

    pub fn load_settings(&self) -> Result<Option<Settings>, ErrorInfo> {
        let conn = self.conn.lock().unwrap();
        let raw: Option<String> =
            conn.query_row("SELECT value FROM settings WHERE key = 'settings'", [], |r| r.get(0)).optional().map_err(db_error)?;
        Ok(raw.and_then(|s| match serde_json::from_str::<Settings>(&s) {
            Ok(v) => Some(v),
            Err(e) => {
                tracing::warn!(error = %e, "settings unreadable, using defaults");
                None
            }
        }))
    }

    /// Small named values that are state rather than settings (e.g. whether the queue is paused).
    pub fn load_value(&self, key: &str) -> Result<Option<String>, ErrorInfo> {
        self.conn
            .lock()
            .unwrap()
            .query_row("SELECT value FROM settings WHERE key = ?1", params![format!("state.{key}")], |r| r.get(0))
            .optional()
            .map_err(db_error)
    }

    pub fn save_value(&self, key: &str, value: &str) -> Result<(), ErrorInfo> {
        self.conn
            .lock()
            .unwrap()
            .execute(
                "INSERT INTO settings (key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                params![format!("state.{key}"), value],
            )
            .map_err(db_error)?;
        Ok(())
    }

    pub fn save_settings(&self, settings: &Settings) -> Result<(), ErrorInfo> {
        let data = serde_json::to_string(settings).map_err(|e| ErrorInfo::new("storage", e.to_string()))?;
        self.conn
            .lock()
            .unwrap()
            .execute(
                "INSERT INTO settings (key, value) VALUES ('settings', ?1) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                params![data],
            )
            .map_err(db_error)?;
        Ok(())
    }
}
