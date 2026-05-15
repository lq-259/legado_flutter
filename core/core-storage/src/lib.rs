//! # core-storage - 存储引擎模块
//!
//! 负责所有数据持久化操作，对应原Legado的`data/entities/`和`help/storage/`。
//! 使用SQLite + rusqlite实现轻量级本地数据库，chrono处理时间相关字段。

pub mod book_dao;
pub mod cache_dao;
pub mod chapter_dao;
pub mod database;
pub mod download_dao;
pub mod models;
pub mod progress_dao;
pub mod replace_rule_dao;
pub mod source_dao;

// 重新导出主要类型，方便上层调用
pub use book_dao::BookDao;
pub use cache_dao::CacheDao;
pub use chapter_dao::ChapterDao;
pub use download_dao::DownloadDao;
pub use models::{
    Book, BookProgress, BookSource, Bookmark, Chapter, DownloadChapter, DownloadTask, ReplaceRule,
};
pub use progress_dao::ProgressDao;
pub use replace_rule_dao::ReplaceRuleDao;
pub use source_dao::SourceDao;

use database::init_database as db_init;

/// 数据库配置
pub struct DatabaseConfig {
    pub path: String,
    pub enable_wal: bool, // 启用 WAL 模式提升并发性能
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            path: "legado.db".to_string(),
            enable_wal: true,
        }
    }
}

/// 存储管理器（统一管理所有 DAO）
pub struct StorageManager {
    conn: rusqlite::Connection,
}

impl StorageManager {
    /// 创建新的存储管理器
    pub fn new(config: DatabaseConfig) -> Result<Self, Box<dyn std::error::Error>> {
        let conn = db_init(&config.path)?;

        // 启用 WAL 模式
        if config.enable_wal {
            conn.pragma_update(None, "journal_mode", "WAL")?;
        }

        Ok(Self { conn })
    }

    /// 获取 BookDao
    pub fn book_dao(&self) -> BookDao<'_> {
        BookDao::new(&self.conn)
    }

    /// 获取 SourceDao
    pub fn source_dao(&mut self) -> SourceDao<'_> {
        SourceDao::new(&mut self.conn)
    }

    /// 获取 ChapterDao
    pub fn chapter_dao(&self) -> ChapterDao<'_> {
        ChapterDao::new(&self.conn)
    }

    /// 获取 ProgressDao
    pub fn progress_dao(&self) -> ProgressDao<'_> {
        ProgressDao::new(&self.conn)
    }

    /// 获取 DownloadDao
    pub fn download_dao(&self) -> DownloadDao<'_> {
        DownloadDao::new(&self.conn)
    }

    /// 获取 ReplaceRuleDao
    pub fn replace_rule_dao(&self) -> ReplaceRuleDao<'_> {
        ReplaceRuleDao::new(&self.conn)
    }

    /// 获取 CacheDao
    pub fn cache_dao(&self) -> cache_dao::CacheDao<'_> {
        cache_dao::CacheDao::new(&self.conn)
    }
}

/// 便捷函数：快速初始化数据库
pub fn init_database(path: &str) -> Result<rusqlite::Connection, Box<dyn std::error::Error>> {
    database::init_database(path).map_err(|e| Box::new(e) as Box<dyn std::error::Error>)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    impl StorageManager {
        fn journal_mode(&self) -> String {
            self.conn
                .pragma_query_value(None, "journal_mode", |row| row.get(0))
                .unwrap()
        }
    }

    #[test]
    fn test_wal_enabled() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir
            .path()
            .join("test_wal.db")
            .to_string_lossy()
            .to_string();
        let config = DatabaseConfig {
            path: db_path,
            enable_wal: true,
        };
        let manager = StorageManager::new(config).unwrap();
        assert_eq!(manager.journal_mode().to_lowercase(), "wal");
    }

    #[test]
    fn test_wal_disabled() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir
            .path()
            .join("test_no_wal.db")
            .to_string_lossy()
            .to_string();
        let config = DatabaseConfig {
            path: db_path,
            enable_wal: false,
        };
        let manager = StorageManager::new(config).unwrap();
        assert_ne!(manager.journal_mode().to_lowercase(), "wal");
    }
}
