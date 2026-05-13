//! # 章节 DAO (Data Access Object)
//!
//! 提供章节相关的数据库操作。
//! 对应原 Legado 的 Chapter 实体操作 (data/entities/Chapter.kt)

use rusqlite::{Connection, Result as SqlResult, params};
use tracing::{debug, info};
use uuid::Uuid;
use chrono::Utc;
use super::models::Chapter;

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
                title = excluded.title,
                url = excluded.url,
                content = excluded.content,
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

    /// 根据 ID 获取章节
    pub fn get_by_id(&self, id: &str) -> SqlResult<Option<Chapter>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, book_id, index_num, title, url, content,
                    is_volume, is_checked, start, end,
                    created_at, updated_at
             FROM chapters WHERE id = ?"
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
             FROM chapters WHERE book_id = ? ORDER BY index_num ASC"
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
             FROM chapters WHERE url = ?"
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
        self.conn.execute("DELETE FROM chapters WHERE id = ?", params![id])?;
        Ok(())
    }

    /// 删除书籍的所有章节
    pub fn delete_by_book(&self, book_id: &str) -> SqlResult<()> {
        info!("删除书籍的所有章节: {}", book_id);
        self.conn.execute("DELETE FROM chapters WHERE book_id = ?", params![book_id])?;
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
