use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use fuzzy_matcher::FuzzyMatcher;
use image::GenericImageView;
use rusqlite::{Connection, params};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

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
    pub content_path: Option<String>,
    pub mime_type: Option<String>,
    pub category: Option<String>,
    /// On-disk thumbnail path for image entries, derived from `content_path`.
    /// Lets a QML frontend render previews via `file://` without shipping bytes
    /// over IPC. `None` for text entries.
    pub thumb_path: Option<String>,
}

fn serialize_ts<S: serde::Serializer>(dt: &DateTime<Utc>, s: S) -> Result<S::Ok, S::Error> {
    s.collect_str(&dt.format("%Y-%m-%dT%H:%M:%S%.3fZ"))
}

pub struct Store {
    conn: Connection,
    max_entries: usize,
    max_image_entries: usize,
    images_dir: Option<PathBuf>,
}

impl Store {
    pub fn open(
        db_path: &Path,
        max_entries: usize,
        max_image_entries: usize,
        images_dir: Option<PathBuf>,
    ) -> Result<Self> {
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent).context("failed to create data directory")?;
        }
        if let Some(dir) = &images_dir {
            std::fs::create_dir_all(dir).context("failed to create images directory")?;
        }
        let conn = Connection::open(db_path).context("failed to open database")?;
        let mut store = Self {
            conn,
            max_entries,
            max_image_entries,
            images_dir,
        };
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
        .context("failed to create clipboard schema")?;

        for migration in &[
            "ALTER TABLE clipboard ADD COLUMN content_path TEXT",
            "ALTER TABLE clipboard ADD COLUMN mime_type TEXT",
            "ALTER TABLE clipboard ADD COLUMN category TEXT",
        ] {
            tx.execute(migration, []).ok();
        }

        tx.execute_batch(
            "CREATE TABLE IF NOT EXISTS categories (
                id    INTEGER PRIMARY KEY AUTOINCREMENT,
                name  TEXT NOT NULL UNIQUE,
                color TEXT
            );
            CREATE INDEX IF NOT EXISTS idx_clipboard_category ON clipboard(category);",
        )
        .context("failed to create categories schema")?;

        let predefined = [
            ("Code", "#7aa2f7"),
            ("Design", "#bb9af7"),
            ("Links", "#73daca"),
            ("Notes", "#e0af68"),
            ("Sensitive", "#f7768e"),
        ];
        for (name, color) in &predefined {
            tx.execute(
                "INSERT OR IGNORE INTO categories (name, color) VALUES (?1, ?2)",
                params![name, color],
            )
            .ok();
        }

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
            return self
                .get_by_hash(&hash)?
                .context("entry not found after update");
        }

        tx.execute(
            "INSERT INTO clipboard (content, content_type, source, hash, timestamp) \
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![content, content_type, source, hash, ts_str],
        )
        .context("failed to insert entry")?;

        tx.commit().context("failed to commit insert")?;
        self.evict_old_text()?;

        self.get_by_hash(&hash)?
            .context("entry not found after insert")
    }

    pub fn insert_image(
        &mut self,
        data: &[u8],
        mime_type: &str,
        source: Option<&str>,
    ) -> Result<ClipboardEntry> {
        let hash = hex::encode(Sha256::digest(data));

        let content_path = self
            .images_dir
            .as_ref()
            .map(|dir| {
                let ext = mime_type.rsplit('/').next().unwrap_or("png");
                let path = dir.join(format!("{}.{}", hash, ext));
                let _ = std::fs::write(&path, data);
                let _ = generate_thumbnail(&path, dir);
                path.to_string_lossy().to_string()
            })
            .unwrap_or_default();

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
            return self
                .get_by_hash(&hash)?
                .context("entry not found after update");
        }

        tx.execute(
            "INSERT INTO clipboard (content, content_type, source, hash, timestamp, \
             content_path, mime_type) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params!["", "image", source, hash, ts_str, content_path, mime_type],
        )
        .context("failed to insert image entry")?;

        tx.commit().context("failed to commit insert")?;
        self.evict_old_images()?;

        self.get_by_hash(&hash)?
            .context("entry not found after insert")
    }

    pub fn get_by_hash(&self, hash: &str) -> Result<Option<ClipboardEntry>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id, content, content_type, source, hash, timestamp, \
                 favorite, content_path, mime_type, category \
                 FROM clipboard WHERE hash = ?1",
            )
            .context("failed to prepare get_by_hash")?;
        Ok(stmt.query_row(params![hash], map_row).ok())
    }

    pub fn list(&self, limit: usize, offset: usize) -> Result<Vec<ClipboardEntry>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id, content, content_type, source, hash, timestamp, \
                 favorite, content_path, mime_type, category \
                 FROM clipboard ORDER BY timestamp DESC LIMIT ?1 OFFSET ?2",
            )
            .context("failed to prepare list")?;
        let entries = stmt
            .query_map(params![limit as i64, offset as i64], map_row)
            .context("failed to prepare list")?
            .filter_map(|r| r.ok())
            .collect();
        Ok(entries)
    }

    pub fn list_by_type(
        &self,
        content_type: &str,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<ClipboardEntry>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id, content, content_type, source, hash, timestamp, \
                 favorite, content_path, mime_type, category \
                 FROM clipboard WHERE content_type = ?1 \
                 ORDER BY timestamp DESC LIMIT ?2 OFFSET ?3",
            )
            .context("failed to prepare list_by_type")?;
        let entries = stmt
            .query_map(params![content_type, limit as i64, offset as i64], map_row)
            .context("failed to query list_by_type")?
            .filter_map(|r| r.ok())
            .collect();
        Ok(entries)
    }

    pub fn list_by_source(
        &self,
        source: &str,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<ClipboardEntry>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id, content, content_type, source, hash, timestamp, \
                 favorite, content_path, mime_type, category \
                 FROM clipboard WHERE source = ?1 \
                 ORDER BY timestamp DESC LIMIT ?2 OFFSET ?3",
            )
            .context("failed to prepare list_by_source")?;
        let entries = stmt
            .query_map(params![source, limit as i64, offset as i64], map_row)
            .context("failed to query list_by_source")?
            .filter_map(|r| r.ok())
            .collect();
        Ok(entries)
    }

    pub fn list_by_category(
        &self,
        category: &str,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<ClipboardEntry>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id, content, content_type, source, hash, timestamp, \
                 favorite, content_path, mime_type, category \
                 FROM clipboard WHERE category = ?1 \
                 ORDER BY timestamp DESC LIMIT ?2 OFFSET ?3",
            )
            .context("failed to prepare list_by_category")?;
        let entries = stmt
            .query_map(params![category, limit as i64, offset as i64], map_row)
            .context("failed to query list_by_category")?
            .filter_map(|r| r.ok())
            .collect();
        Ok(entries)
    }

    pub fn list_sources(&self) -> Result<Vec<String>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT DISTINCT source FROM clipboard \
                 WHERE source IS NOT NULL ORDER BY source",
            )
            .context("failed to prepare list_sources")?;
        let sources = stmt
            .query_map([], |row| row.get::<_, String>(0))
            .context("failed to query sources")?
            .filter_map(|r| r.ok())
            .collect();
        Ok(sources)
    }

    pub fn count_by_type(&self, content_type: &str) -> Result<usize> {
        self.conn
            .query_row(
                "SELECT COUNT(*) FROM clipboard WHERE content_type = ?1",
                params![content_type],
                |row| row.get::<_, i64>(0),
            )
            .map(|c| c as usize)
            .context("failed to count_by_type")
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
                "UPDATE clipboard SET favorite = CASE WHEN favorite = 0 \
                 THEN 1 ELSE 0 END WHERE id = ?1",
                params![id],
            )
            .context("failed to toggle favorite")?;
        Ok(self
            .conn
            .query_row(
                "SELECT favorite FROM clipboard WHERE id = ?1",
                params![id],
                |row| row.get::<_, i64>(0),
            )
            .unwrap_or(0)
            != 0)
    }

    pub fn list_favorites(&self, limit: usize, offset: usize) -> Result<Vec<ClipboardEntry>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id, content, content_type, source, hash, timestamp, \
                 favorite, content_path, mime_type, category \
                 FROM clipboard WHERE favorite = 1 \
                 ORDER BY timestamp DESC LIMIT ?1 OFFSET ?2",
            )
            .context("failed to prepare list_favorites")?;
        let entries = stmt
            .query_map(params![limit as i64, offset as i64], map_row)
            .context("failed to query list_favorites")?
            .filter_map(|r| r.ok())
            .collect();
        Ok(entries)
    }

    pub fn set_category(&mut self, id: i64, category: Option<&str>) -> Result<()> {
        self.conn
            .execute(
                "UPDATE clipboard SET category = ?1 WHERE id = ?2",
                params![category, id],
            )
            .context("failed to set category")?;
        Ok(())
    }

    pub fn delete(&mut self, id: i64) -> Result<()> {
        let entry = self
            .get_by_id(id)?
            .context("entry not found for deletion")?;
        if let Some(path) = &entry.content_path {
            let _ = std::fs::remove_file(path);
        }
        self.conn
            .execute("DELETE FROM clipboard WHERE id = ?1", params![id])
            .context("failed to delete entry")?;
        Ok(())
    }

    pub fn get_by_id(&self, id: i64) -> Result<Option<ClipboardEntry>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id, content, content_type, source, hash, timestamp, \
                 favorite, content_path, mime_type, category \
                 FROM clipboard WHERE id = ?1",
            )
            .context("failed to prepare get_by_id")?;
        Ok(stmt.query_row(params![id], map_row).ok())
    }

    pub fn clear(&mut self) -> Result<()> {
        let paths: Vec<String> = self
            .conn
            .prepare("SELECT content_path FROM clipboard WHERE content_path IS NOT NULL")
            .and_then(|mut stmt| {
                stmt.query_map([], |row| row.get::<_, String>(0))
                    .map(|rows| rows.filter_map(|r| r.ok()).collect())
            })
            .unwrap_or_default();
        for path in &paths {
            let _ = std::fs::remove_file(path);
        }
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

    /// Update eviction caps at runtime (from the settings window). New caps
    /// take effect on the next insert.
    pub fn set_limits(&mut self, max_entries: usize, max_image_entries: usize) {
        self.max_entries = max_entries;
        self.max_image_entries = max_image_entries;
    }

    /// Entry counts as `(total, text, image, favorites)` for the shelf tabs.
    pub fn type_counts(&self) -> Result<(usize, usize, usize, usize)> {
        let one = |sql: &str| -> Result<usize> {
            self.conn
                .query_row(sql, [], |row| row.get::<_, i64>(0))
                .map(|c| c as usize)
                .context("failed to count entries")
        };
        Ok((
            one("SELECT COUNT(*) FROM clipboard")?,
            one("SELECT COUNT(*) FROM clipboard WHERE content_type = 'text'")?,
            one("SELECT COUNT(*) FROM clipboard WHERE content_type = 'image'")?,
            one("SELECT COUNT(*) FROM clipboard WHERE favorite = 1")?,
        ))
    }

    pub fn get_image_data(&self, id: i64) -> Result<Option<Vec<u8>>> {
        let entry = self.get_by_id(id)?;
        match entry {
            Some(e) if e.content_type == "image" => {
                if let Some(path) = &e.content_path {
                    std::fs::read(path)
                        .map(Some)
                        .context("failed to read image file")
                } else {
                    Ok(None)
                }
            }
            _ => Ok(None),
        }
    }

    fn evict_old_text(&mut self) -> Result<()> {
        self.conn
            .execute(
                "DELETE FROM clipboard \
                 WHERE id NOT IN ( \
                     SELECT id FROM clipboard \
                     WHERE content_type != 'image' AND favorite = 0 \
                     ORDER BY timestamp DESC LIMIT ?1 \
                 ) AND content_type != 'image' AND favorite = 0",
                params![self.max_entries as i64],
            )
            .context("failed to evict old text entries")?;
        Ok(())
    }

    fn evict_old_images(&mut self) -> Result<()> {
        let paths: Vec<String> = self
            .conn
            .prepare(
                "SELECT content_path FROM clipboard \
                 WHERE id NOT IN ( \
                     SELECT id FROM clipboard \
                     WHERE content_type = 'image' AND favorite = 0 \
                     ORDER BY timestamp DESC LIMIT ?1 \
                 ) AND content_type = 'image' AND favorite = 0 \
                 AND content_path IS NOT NULL",
            )
            .and_then(|mut stmt| {
                stmt.query_map(params![self.max_image_entries as i64], |row| {
                    row.get::<_, String>(0)
                })
                .map(|rows| rows.filter_map(|r| r.ok()).collect())
            })
            .unwrap_or_default();

        self.conn
            .execute(
                "DELETE FROM clipboard \
                 WHERE id NOT IN ( \
                     SELECT id FROM clipboard \
                     WHERE content_type = 'image' AND favorite = 0 \
                     ORDER BY timestamp DESC LIMIT ?1 \
                 ) AND content_type = 'image' AND favorite = 0",
                params![self.max_image_entries as i64],
            )
            .context("failed to evict old images")?;

        for path in &paths {
            let _ = std::fs::remove_file(path);
        }
        Ok(())
    }

    // --- Categories ---

    pub fn list_categories(&self) -> Result<Vec<(i64, String, Option<String>)>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, name, color FROM categories ORDER BY name")
            .context("failed to prepare list_categories")?;
        let categories = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<String>>(2)?,
                ))
            })
            .context("failed to query categories")?
            .filter_map(|r| r.ok())
            .collect();
        Ok(categories)
    }

    pub fn create_category(&mut self, name: &str, color: Option<&str>) -> Result<i64> {
        self.conn
            .execute(
                "INSERT INTO categories (name, color) VALUES (?1, ?2)",
                params![name, color],
            )
            .context("failed to create category")?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn delete_category(&mut self, name: &str) -> Result<()> {
        self.conn
            .execute(
                "UPDATE clipboard SET category = NULL WHERE category = ?1",
                params![name],
            )
            .context("failed to unset category on entries")?;
        self.conn
            .execute("DELETE FROM categories WHERE name = ?1", params![name])
            .context("failed to delete category")?;
        Ok(())
    }

    pub fn count_by_category(&self) -> Result<Vec<(String, usize)>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT category, COUNT(*) FROM clipboard \
                 WHERE category IS NOT NULL GROUP BY category ORDER BY category",
            )
            .context("failed to prepare count_by_category")?;
        let counts = stmt
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)? as usize))
            })
            .context("failed to query count_by_category")?
            .filter_map(|r| r.ok())
            .collect();
        Ok(counts)
    }
}

fn map_row(row: &rusqlite::Row) -> rusqlite::Result<ClipboardEntry> {
    let ts_str: String = row.get(5)?;
    let timestamp = DateTime::parse_from_rfc3339(&ts_str)
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or_else(|_| Utc::now());
    let content_path: Option<String> = row.get(7)?;
    let thumb_path = content_path.as_deref().and_then(derive_thumb_path);
    Ok(ClipboardEntry {
        id: row.get(0)?,
        content: row.get(1)?,
        content_type: row.get(2)?,
        source: row.get(3)?,
        hash: row.get(4)?,
        timestamp,
        favorite: row.get::<_, i64>(6)? != 0,
        content_path,
        mime_type: row.get(8)?,
        category: row.get(9)?,
        thumb_path,
    })
}

/// Thumbnail path for an image `content_path`, matching the `thumb_{filename}`
/// naming produced by [`generate_thumbnail`]. Returns the deterministic path
/// even if the file is absent (old entries) — the frontend falls back to the
/// full image when it fails to load.
fn derive_thumb_path(content_path: &str) -> Option<String> {
    let p = Path::new(content_path);
    let name = p.file_name()?.to_str()?;
    let parent = p.parent()?;
    Some(
        parent
            .join(format!("thumb_{name}"))
            .to_string_lossy()
            .into_owned(),
    )
}

pub fn generate_thumbnail(image_path: &Path, images_dir: &Path) -> Result<Option<PathBuf>> {
    let img = match image::open(image_path) {
        Ok(img) => img,
        Err(e) => {
            tracing::warn!("failed to open image for thumbnail: {}", e);
            return Ok(None);
        }
    };
    let (w, h) = img.dimensions();
    let thumb_size = 64u32;
    let thumb = if w > h {
        let new_w = (w as f32 * thumb_size as f32 / h as f32) as u32;
        img.resize(new_w, thumb_size, image::imageops::FilterType::CatmullRom)
    } else {
        let new_h = (h as f32 * thumb_size as f32 / w as f32) as u32;
        img.resize(thumb_size, new_h, image::imageops::FilterType::CatmullRom)
    };
    let thumb = thumb.crop_imm(
        0,
        0,
        thumb_size.min(thumb.width()),
        thumb_size.min(thumb.height()),
    );

    let filename = image_path.file_name().unwrap_or_default();
    let thumb_path = images_dir.join(format!("thumb_{}", filename.to_string_lossy()));
    if let Err(e) = thumb.save(&thumb_path) {
        tracing::warn!("failed to save thumbnail: {}", e);
        return Ok(None);
    }
    Ok(Some(thumb_path))
}

impl Store {
    pub fn get_image_thumbnail(&self, id: i64) -> Result<Option<Vec<u8>>> {
        let entry = self.get_by_id(id)?;
        match entry {
            Some(e) if e.content_type == "image" => {
                if let Some(path) = &e.content_path {
                    let p = Path::new(path);
                    let thumb_name = format!(
                        "thumb_{}",
                        p.file_name().unwrap_or_default().to_string_lossy()
                    );
                    let thumb_path = p.parent().map(|parent| parent.join(&thumb_name));
                    if let Some(tp) = thumb_path
                        && tp.exists()
                    {
                        std::fs::read(tp)
                            .map(Some)
                            .context("failed to read thumbnail")
                    } else {
                        Ok(None)
                    }
                } else {
                    Ok(None)
                }
            }
            _ => Ok(None),
        }
    }

    pub fn list_by_type_and_category(
        &self,
        content_type: &str,
        category: &str,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<ClipboardEntry>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id, content, content_type, source, hash, timestamp, \
                 favorite, content_path, mime_type, category \
                 FROM clipboard WHERE content_type = ?1 AND category = ?2 \
                 ORDER BY timestamp DESC LIMIT ?3 OFFSET ?4",
            )
            .context("failed to prepare list_by_type_and_category")?;
        let entries = stmt
            .query_map(
                params![content_type, category, limit as i64, offset as i64],
                map_row,
            )
            .context("failed to query list_by_type_and_category")?
            .filter_map(|r| r.ok())
            .collect();
        Ok(entries)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_db(name: &str) -> (PathBuf, PathBuf) {
        let dir =
            std::env::temp_dir().join(format!("clipvault_test_{}_{}", std::process::id(), name));
        let _ = std::fs::remove_dir_all(&dir);
        let db_path = dir.join("test.db");
        (dir, db_path)
    }

    #[test]
    fn should_insert_and_retrieve_entry() {
        let (_dir, db_path) = tmp_db("insert");
        let mut store = Store::open(&db_path, 100, 50, None).unwrap();
        let entry = store.insert("hello world", "text", Some("test")).unwrap();
        assert_eq!(entry.content, "hello world");
        let list = store.list(10, 0).unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].content, "hello world");
    }

    #[test]
    fn should_deduplicate_same_content() {
        let (_dir, db_path) = tmp_db("dedup");
        let mut store = Store::open(&db_path, 100, 50, None).unwrap();
        store.insert("dup content", "text", None).unwrap();
        store.insert("dup content", "text", None).unwrap();
        assert_eq!(store.count().unwrap(), 1);
    }

    #[test]
    fn should_evict_old_entries() {
        let (_dir, db_path) = tmp_db("evict");
        let mut store = Store::open(&db_path, 3, 50, None).unwrap();
        for i in 0..5 {
            store.insert(&format!("entry {}", i), "text", None).unwrap();
        }
        assert_eq!(store.count().unwrap(), 3);
    }

    #[test]
    fn should_search_entries() {
        let (_dir, db_path) = tmp_db("search");
        let mut store = Store::open(&db_path, 100, 50, None).unwrap();
        store.insert("https://example.com", "text", None).unwrap();
        store.insert("cargo build --release", "text", None).unwrap();
        store.insert("git commit -m 'fix'", "text", None).unwrap();
        let results = store.search("cargo", 10).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].content, "cargo build --release");
    }

    #[test]
    fn should_insert_and_retrieve_image() {
        let (_dir, db_path) = tmp_db("image");
        let images_dir = db_path.parent().unwrap().join("images");
        let mut store = Store::open(&db_path, 100, 50, Some(images_dir)).unwrap();
        let img_data = b"fake-png-bytes";
        let entry = store
            .insert_image(img_data, "image/png", Some("test"))
            .unwrap();
        assert_eq!(entry.content_type, "image");
        assert_eq!(entry.mime_type.as_deref(), Some("image/png"));
        assert!(entry.content_path.is_some());
        assert!(std::path::Path::new(&entry.content_path.unwrap()).exists());
    }

    #[test]
    fn should_not_evict_favorites() {
        let (_dir, db_path) = tmp_db("fav_evict");
        let mut store = Store::open(&db_path, 3, 50, None).unwrap();
        for i in 0..3 {
            store.insert(&format!("entry {}", i), "text", None).unwrap();
        }
        let list = store.list(10, 0).unwrap();
        store.toggle_favorite(list[0].id).unwrap();
        for i in 3..6 {
            store.insert(&format!("entry {}", i), "text", None).unwrap();
        }
        let remaining = store.list(10, 0).unwrap();
        let fav_still_there = remaining.iter().any(|e| e.favorite);
        assert!(fav_still_there, "favorite entry should survive eviction");
    }

    #[test]
    fn should_list_by_type() {
        let (_dir, db_path) = tmp_db("by_type");
        let mut store = Store::open(&db_path, 100, 50, None).unwrap();
        store.insert("text entry", "text", None).unwrap();
        let texts = store.list_by_type("text", 10, 0).unwrap();
        assert_eq!(texts.len(), 1);
        let images = store.list_by_type("image", 10, 0).unwrap();
        assert_eq!(images.len(), 0);
    }

    #[test]
    fn should_list_by_source() {
        let (_dir, db_path) = tmp_db("by_source");
        let mut store = Store::open(&db_path, 100, 50, None).unwrap();
        store
            .insert("from firefox", "text", Some("firefox"))
            .unwrap();
        store
            .insert("from alacritty", "text", Some("alacritty"))
            .unwrap();
        let results = store.list_by_source("firefox", 10, 0).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].content, "from firefox");
    }

    #[test]
    fn should_manage_categories() {
        let (_dir, db_path) = tmp_db("categories");
        let mut store = Store::open(&db_path, 100, 50, None).unwrap();
        let cat_id = store.create_category("Test", Some("#ff0000")).unwrap();
        assert!(cat_id > 0);
        let cats = store.list_categories().unwrap();
        assert!(cats.iter().any(|(_, n, _)| n == "Test"));
        store.delete_category("Test").unwrap();
        let cats = store.list_categories().unwrap();
        assert!(!cats.iter().any(|(_, n, _)| n == "Test"));
    }
}
