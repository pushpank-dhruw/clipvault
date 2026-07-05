use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use fuzzy_matcher::FuzzyMatcher;
use rusqlite::{Connection, params};
use sha2::{Digest, Sha256};
use std::path::Path;

#[derive(Debug, Clone, serde::Serialize)]
pub struct ClipboardEntry {
    pub id: i64,
    pub content: String,
    pub content_type: String,
    pub source: Option<String>,
    pub hash: String,
    #[serde(serialize_with = "serialize_ts")]
    pub timestamp: DateTime<Utc>,
    pub favorite: bool,
}

fn serialize_ts<S: serde::Serializer>(dt: &DateTime<Utc>, s: S) -> Result<S::Ok, S::Error> {
    s.collect_str(&dt.format("%Y-%m-%dT%H:%M:%S%.3fZ"))
}

pub struct Store {
    conn: Connection,
    max_entries: usize,
}

impl Store {
    pub fn open(db_path: &Path, max_entries: usize) -> Result<Self> {
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent).context("failed to create data directory")?;
        }
        let conn = Connection::open(db_path).context("failed to open database")?;
        let mut store = Self { conn, max_entries };
        store.migrate()?;
        Ok(store)
    }

    fn migrate(&mut self) -> Result<()> {
        let tx = self
            .conn
            .transaction()
            .context("failed to begin transaction")?;
        tx.execute_batch(
            "CREATE TABLE IF NOT EXISTS clipboard (
                id          INTEGER PRIMARY KEY AUTOINCREMENT,
                content     TEXT NOT NULL,
                content_type TEXT NOT NULL DEFAULT 'text',
                source      TEXT,
                hash        TEXT NOT NULL UNIQUE,
                timestamp   TEXT NOT NULL,
                favorite    INTEGER NOT NULL DEFAULT 0
            );
            CREATE INDEX IF NOT EXISTS idx_clipboard_timestamp ON clipboard(timestamp DESC);
            CREATE INDEX IF NOT EXISTS idx_clipboard_hash ON clipboard(hash);
            CREATE INDEX IF NOT EXISTS idx_clipboard_favorite ON clipboard(favorite);",
        )
        .context("failed to create schema")?;
        tx.commit().context("failed to commit schema")
    }

    pub fn insert(
        &mut self,
        content: &str,
        content_type: &str,
        source: Option<&str>,
    ) -> Result<ClipboardEntry> {
        let hash = hex::encode(Sha256::digest(content.as_bytes()));
        let timestamp = Utc::now();
        let ts_str = timestamp.to_rfc3339();

        let tx = self
            .conn
            .transaction()
            .context("failed to begin transaction")?;

        let exists: bool = tx
            .query_row(
                "SELECT COUNT(*) FROM clipboard WHERE hash = ?1",
                params![hash],
                |row| row.get::<_, i64>(0),
            )
            .unwrap_or(0)
            > 0;

        if exists {
            tx.execute(
                "UPDATE clipboard SET timestamp = ?1 WHERE hash = ?2",
                params![ts_str, hash],
            )
            .context("failed to update existing entry")?;
            tx.commit().context("failed to commit update")?;
            let entry = self
                .get_by_hash(&hash)?
                .context("entry not found after update")?;
            return Ok(entry);
        }

        tx.execute(
            "INSERT INTO clipboard (content, content_type, source, hash, timestamp) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![content, content_type, source, hash, ts_str],
        )
        .context("failed to insert entry")?;

        tx.commit().context("failed to commit insert")?;
        self.evict_old()?;

        let entry = self
            .get_by_hash(&hash)?
            .context("entry not found after insert")?;
        Ok(entry)
    }

    pub fn get_by_hash(&self, hash: &str) -> Result<Option<ClipboardEntry>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, content, content_type, source, hash, timestamp, favorite FROM clipboard WHERE hash = ?1")
            .context("failed to prepare get_by_hash")?;
        let entry = stmt
            .query_row(params![hash], |row| {
                let ts_str: String = row.get(5)?;
                let timestamp = DateTime::parse_from_rfc3339(&ts_str)
                    .map(|dt| dt.with_timezone(&Utc))
                    .unwrap_or_else(|_| Utc::now());
                Ok(ClipboardEntry {
                    id: row.get(0)?,
                    content: row.get(1)?,
                    content_type: row.get(2)?,
                    source: row.get(3)?,
                    hash: row.get(4)?,
                    timestamp,
                    favorite: row.get::<_, i64>(6)? != 0,
                })
            })
            .ok();
        Ok(entry)
    }

    pub fn list(&self, limit: usize, offset: usize) -> Result<Vec<ClipboardEntry>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, content, content_type, source, hash, timestamp, favorite FROM clipboard ORDER BY timestamp DESC LIMIT ?1 OFFSET ?2")
            .context("failed to prepare list")?;
        let entries = stmt
            .query_map(params![limit as i64, offset as i64], |row| {
                let ts_str: String = row.get(5)?;
                let timestamp = DateTime::parse_from_rfc3339(&ts_str)
                    .map(|dt| dt.with_timezone(&Utc))
                    .unwrap_or_else(|_| Utc::now());
                Ok(ClipboardEntry {
                    id: row.get(0)?,
                    content: row.get(1)?,
                    content_type: row.get(2)?,
                    source: row.get(3)?,
                    hash: row.get(4)?,
                    timestamp,
                    favorite: row.get::<_, i64>(6)? != 0,
                })
            })
            .context("failed to query list")?
            .filter_map(|r| r.ok())
            .collect();
        Ok(entries)
    }

    pub fn search(&self, query: &str, limit: usize) -> Result<Vec<ClipboardEntry>> {
        let all = self.list(usize::MAX, 0)?;
        let matcher = fuzzy_matcher::skim::SkimMatcherV2::default();
        let mut scored: Vec<(i64, &ClipboardEntry)> = all
            .iter()
            .filter_map(|e| {
                matcher
                    .fuzzy_match(&e.content, query)
                    .map(|score| (score, e))
            })
            .collect();
        scored.sort_by_key(|b| std::cmp::Reverse(b.0));
        let entries: Vec<ClipboardEntry> = scored
            .into_iter()
            .take(limit)
            .map(|(_, e)| e.clone())
            .collect();
        Ok(entries)
    }

    pub fn toggle_favorite(&mut self, id: i64) -> Result<bool> {
        self.conn
            .execute(
                "UPDATE clipboard SET favorite = CASE WHEN favorite = 0 THEN 1 ELSE 0 END WHERE id = ?1",
                params![id],
            )
            .context("failed to toggle favorite")?;
        let now_fav: bool = self
            .conn
            .query_row(
                "SELECT favorite FROM clipboard WHERE id = ?1",
                params![id],
                |row| row.get::<_, i64>(0),
            )
            .unwrap_or(0)
            != 0;
        Ok(now_fav)
    }

    pub fn delete(&mut self, id: i64) -> Result<()> {
        self.conn
            .execute("DELETE FROM clipboard WHERE id = ?1", params![id])
            .context("failed to delete entry")?;
        Ok(())
    }

    pub fn clear(&mut self) -> Result<()> {
        self.conn
            .execute("DELETE FROM clipboard", [])
            .context("failed to clear clipboard")?;
        Ok(())
    }

    pub fn count(&self) -> Result<usize> {
        self.conn
            .query_row("SELECT COUNT(*) FROM clipboard", [], |row| {
                row.get::<_, i64>(0)
            })
            .map(|c| c as usize)
            .context("failed to count entries")
    }

    fn evict_old(&mut self) -> Result<()> {
        self.conn
            .execute(
                "DELETE FROM clipboard WHERE id NOT IN (SELECT id FROM clipboard ORDER BY timestamp DESC LIMIT ?1)",
                params![self.max_entries as i64],
            )
            .context("failed to evict old entries")?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_db(name: &str) -> (std::path::PathBuf, std::path::PathBuf) {
        let dir =
            std::env::temp_dir().join(format!("clipvault_test_{}_{}", std::process::id(), name));
        let _ = std::fs::remove_dir_all(&dir);
        let db_path = dir.join("test.db");
        (dir, db_path)
    }

    #[test]
    fn should_insert_and_retrieve_entry() {
        let (dir, db_path) = tmp_db("insert");
        let mut store = Store::open(&db_path, 100).unwrap();

        let entry = store.insert("hello world", "text", Some("test")).unwrap();
        assert_eq!(entry.content, "hello world");

        let list = store.list(10, 0).unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].content, "hello world");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn should_deduplicate_same_content() {
        let (dir, db_path) = tmp_db("dedup");
        let mut store = Store::open(&db_path, 100).unwrap();

        store.insert("dup content", "text", None).unwrap();
        store.insert("dup content", "text", None).unwrap();

        let count = store.count().unwrap();
        assert_eq!(count, 1);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn should_evict_old_entries() {
        let (dir, db_path) = tmp_db("evict");
        let mut store = Store::open(&db_path, 3).unwrap();

        for i in 0..5 {
            store.insert(&format!("entry {}", i), "text", None).unwrap();
        }

        let count = store.count().unwrap();
        assert_eq!(count, 3);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn should_search_entries() {
        let (dir, db_path) = tmp_db("search");
        let mut store = Store::open(&db_path, 100).unwrap();

        store.insert("https://example.com", "text", None).unwrap();
        store.insert("cargo build --release", "text", None).unwrap();
        store.insert("git commit -m 'fix'", "text", None).unwrap();

        let results = store.search("cargo", 10).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].content, "cargo build --release");

        let _ = std::fs::remove_dir_all(&dir);
    }
}
