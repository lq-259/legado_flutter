//! # 阅读进度 DAO (Data Access Object)
//!
//! 提供阅读进度相关的数据库操作。
//! 对应原 Legado 的 BookProgress 实体操作。

use super::models::{BookProgress, Bookmark};
use chrono::Utc;
use rusqlite::{params, Connection, Result as SqlResult};
use tracing::debug;

/// 阅读进度 DAO
pub struct ProgressDao<'a> {
    conn: &'a Connection,
}

impl<'a> ProgressDao<'a> {
    /// 创建新的 ProgressDao
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    /// 保存或更新阅读进度
    pub fn upsert(&self, progress: &BookProgress) -> SqlResult<()> {
        debug!(
            "保存阅读进度: book_id={}, chapter={}",
            progress.book_id, progress.chapter_index
        );

        self.conn.execute(
            "INSERT INTO book_progress (book_id, chapter_index, paragraph_index, offset, read_time, updated_at)
             VALUES (?, ?, ?, ?, ?, ?)
            ON CONFLICT(book_id) DO UPDATE SET
                chapter_index = excluded.chapter_index,
                paragraph_index = excluded.paragraph_index,
                offset = excluded.offset,
                read_time = excluded.read_time,
                updated_at = excluded.updated_at",
            params![
                progress.book_id,
                progress.chapter_index,
                progress.paragraph_index,
                progress.offset,
                progress.read_time,
                progress.updated_at,
            ],
        )?;

        Ok(())
    }

    /// 获取书籍的阅读进度
    pub fn get_by_book(&self, book_id: &str) -> SqlResult<Option<BookProgress>> {
        let mut stmt = self.conn.prepare(
            "SELECT book_id, chapter_index, paragraph_index, offset, read_time, updated_at
             FROM book_progress WHERE book_id = ?",
        )?;

        let mut rows = stmt.query(params![book_id])?;

        if let Some(row) = rows.next()? {
            Ok(Some(progress_from_row(row)?))
        } else {
            Ok(None)
        }
    }

    /// 更新阅读进度（便捷函数）
    pub fn update_progress(
        &self,
        book_id: &str,
        chapter_index: i32,
        paragraph_index: i32,
        offset: i32,
    ) -> SqlResult<()> {
        let now = Utc::now().timestamp();

        // 获取现有进度以累加阅读时长
        let existing = self.get_by_book(book_id)?;
        let read_time = existing.map(|p| p.read_time).unwrap_or(0);

        let progress = BookProgress {
            book_id: book_id.to_string(),
            chapter_index,
            paragraph_index,
            offset,
            read_time,
            updated_at: now,
        };

        self.upsert(&progress)
    }

    /// 增加阅读时长
    pub fn add_read_time(&self, book_id: &str, additional_ms: i64) -> SqlResult<()> {
        self.conn.execute(
            "UPDATE book_progress SET read_time = read_time + ?, updated_at = ? WHERE book_id = ?",
            params![additional_ms, Utc::now().timestamp(), book_id],
        )?;
        Ok(())
    }

    /// 删除阅读进度
    pub fn delete(&self, book_id: &str) -> SqlResult<()> {
        self.conn.execute(
            "DELETE FROM book_progress WHERE book_id = ?",
            params![book_id],
        )?;
        Ok(())
    }

    /// 添加书签
    pub fn add_bookmark(&self, bookmark: &Bookmark) -> SqlResult<()> {
        debug!(
            "添加书签: book_id={}, chapter={}",
            bookmark.book_id, bookmark.chapter_index
        );

        self.conn.execute(
            "INSERT INTO bookmarks (id, book_id, chapter_index, paragraph_index, content, created_at)
             VALUES (?, ?, ?, ?, ?, ?)",
            params![
                bookmark.id,
                bookmark.book_id,
                bookmark.chapter_index,
                bookmark.paragraph_index,
                bookmark.content,
                bookmark.created_at,
            ],
        )?;

        Ok(())
    }

    /// 删除书签
    pub fn delete_bookmark(&self, bookmark_id: &str) -> SqlResult<()> {
        self.conn
            .execute("DELETE FROM bookmarks WHERE id = ?", params![bookmark_id])?;
        Ok(())
    }

    /// 获取书籍的所有书签
    pub fn get_bookmarks(&self, book_id: &str) -> SqlResult<Vec<Bookmark>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, book_id, chapter_index, paragraph_index, content, created_at
             FROM bookmarks WHERE book_id = ? ORDER BY chapter_index ASC, paragraph_index ASC",
        )?;

        let rows = stmt.query_map(params![book_id], bookmark_from_row)?;
        rows.collect()
    }

    /// 创建书签（便捷函数）
    pub fn create_bookmark(
        &self,
        book_id: &str,
        chapter_index: i32,
        paragraph_index: i32,
        content: Option<&str>,
    ) -> SqlResult<Bookmark> {
        let now = Utc::now().timestamp();
        let bookmark = Bookmark {
            id: uuid::Uuid::new_v4().to_string(),
            book_id: book_id.to_string(),
            chapter_index,
            paragraph_index,
            content: content.map(|s| s.to_string()),
            created_at: now,
        };

        self.add_bookmark(&bookmark)?;
        Ok(bookmark)
    }
}

/// 从数据库行转换到 BookProgress 结构体
fn progress_from_row(row: &rusqlite::Row) -> SqlResult<BookProgress> {
    Ok(BookProgress {
        book_id: row.get(0)?,
        chapter_index: row.get(1)?,
        paragraph_index: row.get(2)?,
        offset: row.get(3)?,
        read_time: row.get(4)?,
        updated_at: row.get(5)?,
    })
}

/// 从数据库行转换到 Bookmark 结构体
fn bookmark_from_row(row: &rusqlite::Row) -> SqlResult<Bookmark> {
    Ok(Bookmark {
        id: row.get(0)?,
        book_id: row.get(1)?,
        chapter_index: row.get(2)?,
        paragraph_index: row.get(3)?,
        content: row.get(4)?,
        created_at: row.get(5)?,
    })
}
