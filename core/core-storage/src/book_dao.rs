//! # 书籍 DAO (Data Access Object)
//!
//! 提供书籍相关的数据库操作。
//! 对应原 Legado 的 Book 实体操作 (data/entities/Book.kt)

use super::models::Book;
use chrono::Utc;
use rusqlite::{params, Connection, Result as SqlResult};
use tracing::{debug, info};
use uuid::Uuid;

/// 书籍 DAO
pub struct BookDao<'a> {
    conn: &'a Connection,
}

impl<'a> BookDao<'a> {
    /// 创建新的 BookDao
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    /// 插入或更新书籍
    pub fn upsert(&self, book: &Book) -> SqlResult<()> {
        debug!(
            "插入/更新书籍: {} - {}",
            book.name,
            book.author.as_deref().unwrap_or("")
        );

        self.conn.execute(
            "INSERT INTO books (
                id, source_id, source_name, name, author, cover_url, chapter_count,
                latest_chapter_title, intro, kind, book_url, toc_url, last_check_time, last_check_count,
                total_word_count, can_update, order_time, latest_chapter_time,
                custom_cover_path, custom_info_json, created_at, updated_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(id) DO UPDATE SET
                source_name = excluded.source_name,
                name = excluded.name,
                author = excluded.author,
                cover_url = excluded.cover_url,
                chapter_count = excluded.chapter_count,
                latest_chapter_title = excluded.latest_chapter_title,
                intro = excluded.intro,
                kind = excluded.kind,
                book_url = excluded.book_url,
                toc_url = excluded.toc_url,
                last_check_time = excluded.last_check_time,
                last_check_count = excluded.last_check_count,
                total_word_count = excluded.total_word_count,
                can_update = excluded.can_update,
                order_time = excluded.order_time,
                latest_chapter_time = excluded.latest_chapter_time,
                custom_cover_path = excluded.custom_cover_path,
                custom_info_json = excluded.custom_info_json,
                updated_at = excluded.updated_at",
            params![
                book.id,
                book.source_id,
                book.source_name,
                book.name,
                book.author,
                book.cover_url,
                book.chapter_count,
                book.latest_chapter_title,
                book.intro,
                book.kind,
                book.book_url,
                book.toc_url,
                book.last_check_time,
                book.last_check_count,
                book.total_word_count,
                book.can_update as i32,
                book.order_time,
                book.latest_chapter_time,
                book.custom_cover_path,
                book.custom_info_json,
                book.created_at,
                book.updated_at,
            ],
        )?;

        Ok(())
    }

    /// 根据 ID 获取书籍
    pub fn get_by_id(&self, id: &str) -> SqlResult<Option<Book>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, source_id, source_name, name, author, cover_url, chapter_count,
                    latest_chapter_title, intro, kind, book_url, toc_url, last_check_time, last_check_count,
                    total_word_count, can_update, order_time, latest_chapter_time,
                    custom_cover_path, custom_info_json, created_at, updated_at
             FROM books WHERE id = ?"
        )?;

        let mut rows = stmt.query(params![id])?;

        if let Some(row) = rows.next()? {
            Ok(Some(book_from_row(row)?))
        } else {
            Ok(None)
        }
    }

    /// 获取所有书籍（按排序时间倒序）
    pub fn get_all(&self) -> SqlResult<Vec<Book>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, source_id, source_name, name, author, cover_url, chapter_count,
                    latest_chapter_title, intro, kind, book_url, toc_url, last_check_time, last_check_count,
                    total_word_count, can_update, order_time, latest_chapter_time,
                    custom_cover_path, custom_info_json, created_at, updated_at
             FROM books ORDER BY order_time DESC"
        )?;

        let rows = stmt.query_map([], book_from_row)?;
        rows.collect()
    }

    /// 根据书源 ID 获取书籍
    pub fn get_by_source(&self, source_id: &str) -> SqlResult<Vec<Book>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, source_id, source_name, name, author, cover_url, chapter_count,
                    latest_chapter_title, intro, kind, book_url, toc_url, last_check_time, last_check_count,
                    total_word_count, can_update, order_time, latest_chapter_time,
                    custom_cover_path, custom_info_json, created_at, updated_at
             FROM books WHERE source_id = ? ORDER BY order_time DESC"
        )?;

        let rows = stmt.query_map(params![source_id], book_from_row)?;
        rows.collect()
    }

    /// 删除书籍
    pub fn delete(&self, id: &str) -> SqlResult<()> {
        info!("删除书籍: {}", id);
        self.conn
            .execute("DELETE FROM books WHERE id = ?", params![id])?;
        // 章节会因外键级联删除
        Ok(())
    }

    /// 搜索书籍
    pub fn search(&self, keyword: &str) -> SqlResult<Vec<Book>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, source_id, source_name, name, author, cover_url, chapter_count,
                    latest_chapter_title, intro, kind, book_url, toc_url, last_check_time, last_check_count,
                    total_word_count, can_update, order_time, latest_chapter_time,
                    custom_cover_path, custom_info_json, created_at, updated_at
             FROM books 
             WHERE name LIKE ? OR author LIKE ?
             ORDER BY order_time DESC"
        )?;

        let pattern = format!("%{}%", keyword);
        let rows = stmt.query_map(params![pattern, pattern], book_from_row)?;
        rows.collect()
    }

    /// 创建新书籍（便捷函数）
    pub fn create(
        &self,
        source_id: &str,
        source_name: Option<&str>,
        name: &str,
        author: Option<&str>,
    ) -> SqlResult<Book> {
        let now = Utc::now().timestamp();
        let book = Book {
            id: Uuid::new_v4().to_string(),
            source_id: source_id.to_string(),
            source_name: source_name.map(|s| s.to_string()),
            name: name.to_string(),
            author: author.map(|s| s.to_string()),
            cover_url: None,
            chapter_count: 0,
            latest_chapter_title: None,
            intro: None,
            kind: None,
            book_url: None,
            toc_url: None,
            last_check_time: None,
            last_check_count: 0,
            total_word_count: 0,
            can_update: true,
            order_time: now,
            latest_chapter_time: None,
            custom_cover_path: None,
            custom_info_json: None,
            created_at: now,
            updated_at: now,
        };

        self.upsert(&book)?;
        Ok(book)
    }
}

/// 从数据库行转换到 Book 结构体
fn book_from_row(row: &rusqlite::Row) -> SqlResult<Book> {
    Ok(Book {
        id: row.get(0)?,
        source_id: row.get(1)?,
        source_name: row.get(2)?,
        name: row.get(3)?,
        author: row.get(4)?,
        cover_url: row.get(5)?,
        chapter_count: row.get(6)?,
        latest_chapter_title: row.get(7)?,
        intro: row.get(8)?,
        kind: row.get(9)?,
        book_url: row.get(10)?,
        toc_url: row.get(11)?,
        last_check_time: row.get(12)?,
        last_check_count: row.get(13)?,
        total_word_count: row.get(14)?,
        can_update: row.get::<_, i32>(15)? != 0,
        order_time: row.get(16)?,
        latest_chapter_time: row.get(17)?,
        custom_cover_path: row.get(18)?,
        custom_info_json: row.get(19)?,
        created_at: row.get(20)?,
        updated_at: row.get(21)?,
    })
}
