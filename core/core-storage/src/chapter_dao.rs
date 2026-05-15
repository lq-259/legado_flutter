//! # 章节 DAO (Data Access Object)
//!
//! 提供章节相关的数据库操作。
//! 对应原 Legado 的 Chapter 实体操作 (data/entities/Chapter.kt)

use super::models::Chapter;
use chrono::Utc;
use rusqlite::{params, Connection, Result as SqlResult};
use std::collections::HashMap;
use tracing::{debug, info};
use uuid::Uuid;

/// 章节 DAO
pub struct ChapterDao<'a> {
    conn: &'a Connection,
}

impl<'a> ChapterDao<'a> {
    /// 创建新的 ChapterDao
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    /// 插入或更新章节
    pub fn upsert(&self, chapter: &Chapter) -> SqlResult<()> {
        debug!("插入/更新章节: {} - {}", chapter.title, chapter.url);

        self.conn.execute(
            "INSERT INTO chapters (
                id, book_id, index_num, title, url, content,
                is_volume, is_checked, start, end,
                created_at, updated_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(id) DO UPDATE SET
                index_num = excluded.index_num,
                title = excluded.title,
                url = excluded.url,
                content = COALESCE(excluded.content, content),
                is_volume = excluded.is_volume,
                is_checked = excluded.is_checked,
                start = excluded.start,
                end = excluded.end,
                updated_at = excluded.updated_at",
            params![
                chapter.id,
                chapter.book_id,
                chapter.index_num,
                chapter.title,
                chapter.url,
                chapter.content,
                chapter.is_volume as i32,
                chapter.is_checked as i32,
                chapter.start,
                chapter.end,
                chapter.created_at,
                chapter.updated_at,
            ],
        )?;

        Ok(())
    }

    pub fn replace_by_book_preserving_content(
        &self,
        book_id: &str,
        chapters: &[Chapter],
    ) -> SqlResult<()> {
        let existing = self.get_by_book(book_id)?;
        let content_by_url: HashMap<String, Option<String>> = existing
            .into_iter()
            .map(|chapter| (chapter.url, chapter.content))
            .collect();

        self.conn.execute_batch("BEGIN")?;
        let result = (|| -> SqlResult<()> {
            self.conn
                .execute("DELETE FROM chapters WHERE book_id = ?", params![book_id])?;
            for chapter in chapters {
                let mut chapter = chapter.clone();
                if chapter.content.is_none() {
                    chapter.content = content_by_url.get(&chapter.url).cloned().flatten();
                }
                self.upsert(&chapter)?;
            }
            Ok(())
        })();
        match result {
            Ok(()) => self.conn.execute_batch("COMMIT"),
            Err(e) => {
                let _ = self.conn.execute_batch("ROLLBACK");
                Err(e)
            }
        }
    }

    /// 根据 ID 获取章节
    pub fn get_by_id(&self, id: &str) -> SqlResult<Option<Chapter>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, book_id, index_num, title, url, content,
                    is_volume, is_checked, start, end,
                    created_at, updated_at
             FROM chapters WHERE id = ?",
        )?;

        let mut rows = stmt.query(params![id])?;

        if let Some(row) = rows.next()? {
            Ok(Some(chapter_from_row(row)?))
        } else {
            Ok(None)
        }
    }

    /// 获取书籍的所有章节
    pub fn get_by_book(&self, book_id: &str) -> SqlResult<Vec<Chapter>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, book_id, index_num, title, url, content,
                    is_volume, is_checked, start, end,
                    created_at, updated_at
             FROM chapters WHERE book_id = ? ORDER BY index_num ASC",
        )?;

        let rows = stmt.query_map(params![book_id], chapter_from_row)?;
        rows.collect()
    }

    /// 根据 URL 获取章节
    pub fn get_by_url(&self, url: &str) -> SqlResult<Option<Chapter>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, book_id, index_num, title, url, content,
                    is_volume, is_checked, start, end,
                    created_at, updated_at
             FROM chapters WHERE url = ?",
        )?;

        let mut rows = stmt.query(params![url])?;

        if let Some(row) = rows.next()? {
            Ok(Some(chapter_from_row(row)?))
        } else {
            Ok(None)
        }
    }

    /// 更新章节内容
    pub fn update_content(&self, chapter_id: &str, content: &str) -> SqlResult<()> {
        self.conn.execute(
            "UPDATE chapters SET content = ?, updated_at = ? WHERE id = ?",
            params![content, Utc::now().timestamp(), chapter_id],
        )?;
        Ok(())
    }

    /// 删除章节
    pub fn delete(&self, id: &str) -> SqlResult<()> {
        info!("删除章节: {}", id);
        self.conn
            .execute("DELETE FROM chapters WHERE id = ?", params![id])?;
        Ok(())
    }

    /// 删除书籍的所有章节
    pub fn delete_by_book(&self, book_id: &str) -> SqlResult<()> {
        info!("删除书籍的所有章节: {}", book_id);
        self.conn
            .execute("DELETE FROM chapters WHERE book_id = ?", params![book_id])?;
        Ok(())
    }

    /// 创建新章节（便捷函数）
    pub fn create(
        &self,
        book_id: &str,
        index_num: i32,
        title: &str,
        url: &str,
    ) -> SqlResult<Chapter> {
        let now = Utc::now().timestamp();
        let chapter = Chapter {
            id: Uuid::new_v4().to_string(),
            book_id: book_id.to_string(),
            index_num,
            title: title.to_string(),
            url: url.to_string(),
            content: None,
            is_volume: false,
            is_checked: false,
            start: 0,
            end: 0,
            created_at: now,
            updated_at: now,
        };

        self.upsert(&chapter)?;
        Ok(chapter)
    }
}

/// 从数据库行转换到 Chapter 结构体
fn chapter_from_row(row: &rusqlite::Row) -> SqlResult<Chapter> {
    Ok(Chapter {
        id: row.get(0)?,
        book_id: row.get(1)?,
        index_num: row.get(2)?,
        title: row.get(3)?,
        url: row.get(4)?,
        content: row.get(5)?,
        is_volume: row.get::<_, i32>(6)? != 0,
        is_checked: row.get::<_, i32>(7)? != 0,
        start: row.get(8)?,
        end: row.get(9)?,
        created_at: row.get(10)?,
        updated_at: row.get(11)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn setup() -> (TempDir, Connection) {
        let dir = TempDir::new().unwrap();
        let db_path = dir.path().join("test.db");
        let conn = crate::database::init_database(db_path.to_str().unwrap()).unwrap();
        conn.execute(
            "INSERT INTO book_sources (id, name, url, created_at, updated_at) VALUES ('source1', 'Source', 'https://example.com', 1, 1)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO books (id, source_id, source_name, name, order_time, created_at, updated_at) VALUES ('book1', 'source1', 'Source', 'Book', 1, 1, 1)",
            [],
        )
        .unwrap();
        (dir, conn)
    }

    fn chapter(id: &str, index: i32, url: &str, content: Option<&str>) -> Chapter {
        Chapter {
            id: id.to_string(),
            book_id: "book1".to_string(),
            index_num: index,
            title: format!("Chapter {index}"),
            url: url.to_string(),
            content: content.map(str::to_string),
            is_volume: false,
            is_checked: false,
            start: 0,
            end: 0,
            created_at: 1,
            updated_at: 1,
        }
    }

    #[test]
    fn test_upsert_updates_index_num() {
        let (_dir, conn) = setup();
        let dao = ChapterDao::new(&conn);
        dao.upsert(&chapter("ch1", 0, "/a", None)).unwrap();
        dao.upsert(&chapter("ch1", 3, "/a", None)).unwrap();
        let updated = dao.get_by_id("ch1").unwrap().unwrap();
        assert_eq!(updated.index_num, 3);
    }

    #[test]
    fn test_replace_by_book_removes_stale_and_preserves_content_by_url() {
        let (_dir, conn) = setup();
        let dao = ChapterDao::new(&conn);
        dao.upsert(&chapter("old1", 0, "/keep", Some("cached")))
            .unwrap();
        dao.upsert(&chapter("old2", 1, "/stale", Some("stale")))
            .unwrap();
        dao.replace_by_book_preserving_content(
            "book1",
            &[
                chapter("new1", 0, "/keep", None),
                chapter("new2", 1, "/new", None),
            ],
        )
        .unwrap();
        let chapters = dao.get_by_book("book1").unwrap();
        assert_eq!(chapters.len(), 2);
        assert_eq!(chapters[0].url, "/keep");
        assert_eq!(chapters[0].content.as_deref(), Some("cached"));
        assert_eq!(chapters[1].url, "/new");
    }
}
