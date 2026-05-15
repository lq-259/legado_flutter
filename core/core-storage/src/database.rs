//! # 数据库模块
//!
//! 负责 SQLite 数据库的初始化、表创建和迁移。
//! 对应原 Legado 的数据库初始化逻辑 (help/storage/)。

use rusqlite::{Connection, Result as SqlResult};
use tracing::{debug, info, warn};

/// 数据库版本（用于迁移，通过 PRAGMA user_version 持久化）
const DB_VERSION: i32 = 8;

/// 初始化数据库
/// 创建所有必要的表，如果数据库已存在则检查是否需要迁移
pub fn init_database(db_path: &str) -> SqlResult<Connection> {
    info!("初始化数据库: {}", db_path);

    // 确保目录存在
    if let Some(parent) = std::path::Path::new(db_path).parent() {
        if !parent.exists() {
            std::fs::create_dir_all(parent).map_err(|e| {
                rusqlite::Error::SqliteFailure(
                    rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_CANTOPEN),
                    Some(e.to_string()),
                )
            })?;
        }
    }

    let conn = Connection::open(db_path)?;

    // 启用外键约束
    conn.execute("PRAGMA foreign_keys = ON", [])?;

    // 检查数据库版本（使用 PRAGMA user_version）
    let version = get_db_version(&conn)?;
    debug!("当前数据库版本: {}", version);

    // 创建或迁移表
    if version == 0 {
        create_tables(&conn)?;
        set_db_version(&conn, DB_VERSION)?;
    } else if version < DB_VERSION {
        migrate_database(&conn, version, DB_VERSION)?;
    } else if version > DB_VERSION {
        warn!(
            "数据库版本 {} 高于当前版本 {}，跳过迁移",
            version, DB_VERSION
        );
    }

    info!("数据库初始化完成");
    Ok(conn)
}

/// 创建所有表
pub fn create_tables(conn: &Connection) -> SqlResult<()> {
    info!("创建数据库表...");

    // 应用设置表
    conn.execute(
        "CREATE TABLE IF NOT EXISTS app_settings (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL
        )",
        [],
    )?;

    // 书源表 (对应原 Legado 的 BookSource 实体)
    conn.execute(
        "CREATE TABLE IF NOT EXISTS book_sources (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            url TEXT NOT NULL UNIQUE,
            source_type INTEGER DEFAULT 0,
            group_name TEXT,
            enabled INTEGER DEFAULT 1,
            custom_order INTEGER DEFAULT 0,
            weight INTEGER DEFAULT 0,
            
            -- 规则（JSON 格式存储）
            rule_search TEXT,
            rule_book_info TEXT,
            rule_toc TEXT,
            rule_content TEXT,
            
            -- 其他配置
            login_url TEXT,
            header TEXT,
            js_lib TEXT,
            book_url_pattern TEXT,
            rule_explore TEXT,
            explore_url TEXT,
            enabled_explore INTEGER DEFAULT 1,
            last_update_time INTEGER DEFAULT 0,
            book_source_comment TEXT,
            
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL
        )",
        [],
    )?;

    // 书籍表 (对应原 Legado 的 Book 实体)
    conn.execute(
        "CREATE TABLE IF NOT EXISTS books (
            id TEXT PRIMARY KEY,
            source_id TEXT NOT NULL,
            source_name TEXT,
            name TEXT NOT NULL,
            author TEXT,
            cover_url TEXT,
            chapter_count INTEGER DEFAULT 0,
            latest_chapter_title TEXT,
            intro TEXT,
            kind TEXT,
            book_url TEXT,
            toc_url TEXT,
            last_check_time INTEGER,
            last_check_count INTEGER DEFAULT 0,
            total_word_count INTEGER DEFAULT 0,
            can_update INTEGER DEFAULT 1,
            order_time INTEGER NOT NULL,
            latest_chapter_time INTEGER,
            custom_cover_path TEXT,
            custom_info_json TEXT,
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL,
            FOREIGN KEY (source_id) REFERENCES book_sources(id)
        )",
        [],
    )?;

    // 章节表
    conn.execute(
        "CREATE TABLE IF NOT EXISTS chapters (
            id TEXT PRIMARY KEY,
            book_id TEXT NOT NULL,
            index_num INTEGER NOT NULL,
            title TEXT NOT NULL,
            url TEXT NOT NULL,
            content TEXT,
            is_volume INTEGER DEFAULT 0,
            is_checked INTEGER DEFAULT 0,
            start INTEGER DEFAULT 0,
            end INTEGER DEFAULT 0,
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL,
            FOREIGN KEY (book_id) REFERENCES books(id) ON DELETE CASCADE
        )",
        [],
    )?;

    // 阅读进度表
    conn.execute(
        "CREATE TABLE IF NOT EXISTS book_progress (
            book_id TEXT PRIMARY KEY,
            chapter_index INTEGER DEFAULT 0,
            paragraph_index INTEGER DEFAULT 0,
            offset INTEGER DEFAULT 0,
            read_time INTEGER DEFAULT 0,
            updated_at INTEGER NOT NULL,
            FOREIGN KEY (book_id) REFERENCES books(id) ON DELETE CASCADE
        )",
        [],
    )?;

    // 书签表
    conn.execute(
        "CREATE TABLE IF NOT EXISTS bookmarks (
            id TEXT PRIMARY KEY,
            book_id TEXT NOT NULL,
            chapter_index INTEGER NOT NULL,
            paragraph_index INTEGER DEFAULT 0,
            content TEXT,
            created_at INTEGER NOT NULL,
            FOREIGN KEY (book_id) REFERENCES books(id) ON DELETE CASCADE
        )",
        [],
    )?;

    // 替换规则表
    conn.execute(
        "CREATE TABLE IF NOT EXISTS replace_rules (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            pattern TEXT NOT NULL,
            replacement TEXT NOT NULL,
            enabled INTEGER DEFAULT 1,
            scope INTEGER DEFAULT 0,
            sort_number INTEGER DEFAULT 0,
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL
        )",
        [],
    )?;

    // 下载任务表
    conn.execute(
        "CREATE TABLE IF NOT EXISTS download_tasks (
            id TEXT PRIMARY KEY,
            book_id TEXT NOT NULL,
            book_name TEXT NOT NULL,
            cover_url TEXT,
            total_chapters INTEGER DEFAULT 0,
            downloaded_chapters INTEGER DEFAULT 0,
            status INTEGER DEFAULT 0,
            total_size INTEGER DEFAULT 0,
            downloaded_size INTEGER DEFAULT 0,
            error_message TEXT,
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL,
            FOREIGN KEY (book_id) REFERENCES books(id) ON DELETE CASCADE
        )",
        [],
    )?;

    // 下载章节记录表
    conn.execute(
        "CREATE TABLE IF NOT EXISTS download_chapters (
            id TEXT PRIMARY KEY,
            task_id TEXT NOT NULL,
            chapter_id TEXT NOT NULL,
            chapter_index INTEGER NOT NULL,
            chapter_title TEXT NOT NULL,
            status INTEGER DEFAULT 0,
            file_path TEXT,
            file_size INTEGER DEFAULT 0,
            error_message TEXT,
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL,
            FOREIGN KEY (task_id) REFERENCES download_tasks(id) ON DELETE CASCADE
        )",
        [],
    )?;

    // 缓存表
    conn.execute(
        "CREATE TABLE IF NOT EXISTS legacy_cache (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL,
            updated_at INTEGER NOT NULL
        )",
        [],
    )?;

    conn.execute(
        "CREATE TABLE IF NOT EXISTS sync_log (
            id TEXT PRIMARY KEY,
            sync_type TEXT NOT NULL,
            sync_data TEXT,
            status INTEGER DEFAULT 0,
            error_message TEXT,
            started_at INTEGER NOT NULL,
            completed_at INTEGER,
            created_at INTEGER NOT NULL
        )",
        [],
    )?;

    // 创建索引
    create_indices(conn)?;

    info!("数据库表创建完成");
    Ok(())
}

/// 创建索引
fn create_indices(conn: &Connection) -> SqlResult<()> {
    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_books_source_id ON books(source_id)",
        [],
    )?;
    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_chapters_book_id ON chapters(book_id)",
        [],
    )?;
    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_chapters_index ON chapters(book_id, index_num)",
        [],
    )?;
    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_bookmarks_book_id ON bookmarks(book_id)",
        [],
    )?;
    Ok(())
}

/// 获取数据库版本（通过 PRAGMA user_version）
fn get_db_version(conn: &Connection) -> SqlResult<i32> {
    conn.pragma_query_value(None, "user_version", |row| row.get(0))
}

/// 设置数据库版本（通过 PRAGMA user_version）
fn set_db_version(conn: &Connection, version: i32) -> SqlResult<()> {
    conn.pragma_update(None, "user_version", version)?;
    Ok(())
}

/// 数据库迁移 — 按版本逐步迁移，包装在事务中
fn migrate_database(conn: &Connection, from_version: i32, to_version: i32) -> SqlResult<()> {
    info!("数据库迁移: {} -> {}", from_version, to_version);
    conn.execute_batch("BEGIN")?;
    let result = (|| -> SqlResult<()> {
        for v in (from_version + 1)..=to_version {
            debug!("执行版本 {} 迁移", v);
            match v {
                1 => migrate_v1(conn)?,
                2 => migrate_v2(conn)?,
                3 => migrate_v3(conn)?,
                4 => migrate_v4(conn)?,
                5 => migrate_v5(conn)?,
                6 => migrate_v6(conn)?,
                7 => migrate_v7(conn)?,
                8 => migrate_v8(conn)?,
                _ => {
                    return Err(rusqlite::Error::SqliteFailure(
                        rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_ERROR),
                        Some(format!("未知的数据库版本: {}", v)),
                    ));
                }
            }
        }
        set_db_version(conn, to_version)?;
        Ok(())
    })();
    match result {
        Ok(()) => {
            conn.execute_batch("COMMIT")?;
            info!("数据库迁移完成");
            Ok(())
        }
        Err(e) => {
            let _ = conn.execute_batch("ROLLBACK");
            warn!("数据库迁移失败，已回滚: {}", e);
            Err(e)
        }
    }
}

/// 版本 1 迁移：创建初始表结构
fn migrate_v1(conn: &Connection) -> SqlResult<()> {
    create_tables(conn)?;
    Ok(())
}

/// 版本 2 迁移：示例增量迁移（添加 sync_log 表用于同步日志）
fn migrate_v2(conn: &Connection) -> SqlResult<()> {
    conn.execute(
        "CREATE TABLE IF NOT EXISTS sync_log (
            id TEXT PRIMARY KEY,
            sync_type TEXT NOT NULL,
            sync_data TEXT,
            status INTEGER DEFAULT 0,
            error_message TEXT,
            started_at INTEGER NOT NULL,
            completed_at INTEGER,
            created_at INTEGER NOT NULL
        )",
        [],
    )?;
    Ok(())
}

/// 版本 4 迁移：添加 book_url 列
fn migrate_v4(conn: &Connection) -> SqlResult<()> {
    let has_book_url: bool = conn.query_row(
        "SELECT COUNT(*) > 0 FROM pragma_table_info('books') WHERE name = 'book_url'",
        [],
        |row| row.get(0),
    )?;
    if has_book_url {
        return Ok(());
    }
    conn.execute("ALTER TABLE books ADD COLUMN book_url TEXT", [])?;
    Ok(())
}

/// 版本 3 迁移：添加下载任务表
fn migrate_v3(conn: &Connection) -> SqlResult<()> {
    conn.execute(
        "CREATE TABLE IF NOT EXISTS download_tasks (
            id TEXT PRIMARY KEY,
            book_id TEXT NOT NULL,
            book_name TEXT NOT NULL,
            cover_url TEXT,
            total_chapters INTEGER DEFAULT 0,
            downloaded_chapters INTEGER DEFAULT 0,
            status INTEGER DEFAULT 0,
            total_size INTEGER DEFAULT 0,
            downloaded_size INTEGER DEFAULT 0,
            error_message TEXT,
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL,
            FOREIGN KEY (book_id) REFERENCES books(id) ON DELETE CASCADE
        )",
        [],
    )?;
    conn.execute(
        "CREATE TABLE IF NOT EXISTS download_chapters (
            id TEXT PRIMARY KEY,
            task_id TEXT NOT NULL,
            chapter_id TEXT NOT NULL,
            chapter_index INTEGER NOT NULL,
            chapter_title TEXT NOT NULL,
            status INTEGER DEFAULT 0,
            file_path TEXT,
            file_size INTEGER DEFAULT 0,
            error_message TEXT,
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL,
            FOREIGN KEY (task_id) REFERENCES download_tasks(id) ON DELETE CASCADE
        )",
        [],
    )?;
    Ok(())
}

/// 版本 5 迁移：添加缓存表
fn migrate_v5(conn: &Connection) -> SqlResult<()> {
    conn.execute(
        "CREATE TABLE IF NOT EXISTS legacy_cache (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL,
            updated_at INTEGER NOT NULL
        )",
        [],
    )?;
    Ok(())
}

/// 版本 6 迁移：添加探索规则相关列
fn migrate_v6(conn: &Connection) -> SqlResult<()> {
    create_tables(conn)?;
    for (col, col_type) in [
        ("rule_explore", "TEXT"),
        ("explore_url", "TEXT"),
        ("enabled_explore", "INTEGER DEFAULT 1"),
        ("last_update_time", "INTEGER DEFAULT 0"),
        ("book_source_comment", "TEXT"),
    ] {
        let has_col: bool = conn.query_row(
            &format!(
                "SELECT COUNT(*) > 0 FROM pragma_table_info('book_sources') WHERE name = '{}'",
                col
            ),
            [],
            |row| row.get(0),
        )?;
        if !has_col {
            conn.execute(
                &format!("ALTER TABLE book_sources ADD COLUMN {} {}", col, col_type),
                [],
            )?;
        }
    }
    Ok(())
}

/// 版本 7 迁移：为 books 表添加 toc_url 列
fn migrate_v7(conn: &Connection) -> SqlResult<()> {
    let has_toc_url: bool = conn.query_row(
        "SELECT COUNT(*) > 0 FROM pragma_table_info('books') WHERE name = 'toc_url'",
        [],
        |row| row.get(0),
    )?;
    if has_toc_url {
        return Ok(());
    }
    conn.execute("ALTER TABLE books ADD COLUMN toc_url TEXT", [])?;
    Ok(())
}

/// 版本 8 迁移：为 book_sources 表添加 book_url_pattern 列
fn migrate_v8(conn: &Connection) -> SqlResult<()> {
    create_tables(conn)?;
    let has_book_url_pattern: bool = conn.query_row(
        "SELECT COUNT(*) > 0 FROM pragma_table_info('book_sources') WHERE name = 'book_url_pattern'",
        [],
        |row| row.get(0),
    )?;
    if has_book_url_pattern {
        return Ok(());
    }
    conn.execute(
        "ALTER TABLE book_sources ADD COLUMN book_url_pattern TEXT",
        [],
    )?;
    Ok(())
}

/// 获取数据库连接（便捷函数）
pub fn get_connection(db_path: &str) -> SqlResult<Connection> {
    let conn = Connection::open(db_path)?;
    conn.execute("PRAGMA foreign_keys = ON", [])?;
    Ok(conn)
}

/// 执行 SQL 文件（用于初始化或迁移）
pub fn execute_sql_file(conn: &Connection, sql: &str) -> SqlResult<()> {
    conn.execute_batch(sql)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_database_init() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir
            .path()
            .join("test.db")
            .to_string_lossy()
            .to_string();

        let conn = init_database(&db_path).unwrap();

        // 验证表是否存在（含 sync_log）
        let count: i32 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table'",
                [],
                |row| row.get(0),
            )
            .unwrap();

        assert!(count >= 8); // 至少 8 个表（含 sync_log）

        let version = get_db_version(&conn).unwrap();
        assert_eq!(version, DB_VERSION);

        let has_book_url: bool = conn
            .query_row(
                "SELECT COUNT(*) > 0 FROM pragma_table_info('books') WHERE name = 'book_url'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(has_book_url, "fresh database should include book_url");
    }

    #[test]
    fn test_migration_from_v3_adds_book_url() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir
            .path()
            .join("test_migrate_v3.db")
            .to_string_lossy()
            .to_string();

        let conn = Connection::open(&db_path).unwrap();
        conn.execute("PRAGMA foreign_keys = ON", []).unwrap();
        conn.execute(
            "CREATE TABLE books (
                id TEXT PRIMARY KEY,
                source_id TEXT NOT NULL,
                source_name TEXT,
                name TEXT NOT NULL,
                author TEXT,
                cover_url TEXT,
                chapter_count INTEGER DEFAULT 0,
                latest_chapter_title TEXT,
                intro TEXT,
                kind TEXT,
                last_check_time INTEGER,
                last_check_count INTEGER DEFAULT 0,
                total_word_count INTEGER DEFAULT 0,
                can_update INTEGER DEFAULT 1,
                order_time INTEGER NOT NULL,
                latest_chapter_time INTEGER,
                custom_cover_path TEXT,
                custom_info_json TEXT,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL
            )",
            [],
        )
        .unwrap();
        conn.pragma_update(None, "user_version", 3_i32).unwrap();
        drop(conn);

        let migrated = init_database(&db_path).unwrap();
        let has_book_url: bool = migrated
            .query_row(
                "SELECT COUNT(*) > 0 FROM pragma_table_info('books') WHERE name = 'book_url'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(has_book_url, "v3 migration should add book_url");

        let version = get_db_version(&migrated).unwrap();
        assert_eq!(version, DB_VERSION);
    }

    #[test]
    fn test_migration_from_v1_to_v2() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir
            .path()
            .join("test_migrate.db")
            .to_string_lossy()
            .to_string();

        // 创建 v1 数据库
        let conn = Connection::open(&db_path).unwrap();
        conn.execute("PRAGMA foreign_keys = ON", []).unwrap();
        // Simulate a v1-era schema by creating tables and then dropping sync_log
        // which was added in v2 migration.
        create_tables(&conn).unwrap();
        conn.execute("DROP TABLE IF EXISTS sync_log", []).unwrap();
        conn.pragma_update(None, "user_version", 1_i32).unwrap();
        drop(conn);

        // 重新打开，触发迁移
        let conn2 = init_database(&db_path).unwrap();

        // 验证 v2 有 sync_log 表
        let has_sync_log: bool = conn2
            .query_row(
                "SELECT COUNT(*) > 0 FROM sqlite_master WHERE type='table' AND name='sync_log'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(has_sync_log, "v2 should have sync_log table");

        // 验证版本号
        let version = get_db_version(&conn2).unwrap();
        assert_eq!(version, DB_VERSION);
    }

    #[test]
    fn test_book_dao_crud() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir
            .path()
            .join("test_book_dao.db")
            .to_string_lossy()
            .to_string();
        let conn = init_database(&db_path).unwrap();
        // Insert required book_source for foreign key constraint
        conn.execute(
            "INSERT INTO book_sources (id, name, url, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params!["source1", "Test Source", "https://example.com", chrono::Utc::now().timestamp(), chrono::Utc::now().timestamp()],
        ).unwrap();
        let dao = crate::book_dao::BookDao::new(&conn);

        let book = dao
            .create("source1", Some("Test Source"), "Test Book", Some("Author"))
            .unwrap();
        assert_eq!(book.name, "Test Book");
        assert_eq!(book.source_id, "source1");

        let retrieved = dao.get_by_id(&book.id).unwrap().unwrap();
        assert_eq!(retrieved.author.as_deref(), Some("Author"));

        let mut updated = book.clone();
        updated.name = "Updated Book".to_string();
        updated.author = Some("New Author".to_string());
        dao.upsert(&updated).unwrap();
        let r2 = dao.get_by_id(&book.id).unwrap().unwrap();
        assert_eq!(r2.name, "Updated Book");

        assert_eq!(dao.get_all().unwrap().len(), 1);
        assert_eq!(dao.search("Updated").unwrap().len(), 1);
        assert_eq!(dao.get_by_source("source1").unwrap().len(), 1);

        dao.delete(&book.id).unwrap();
        assert!(dao.get_by_id(&book.id).unwrap().is_none());
    }

    #[test]
    fn test_source_dao_crud() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir
            .path()
            .join("test_source_dao.db")
            .to_string_lossy()
            .to_string();
        let mut conn = init_database(&db_path).unwrap();
        let dao = crate::source_dao::SourceDao::new(&mut conn);

        let source = dao.create("Test Source", "https://example.com").unwrap();
        assert_eq!(source.name, "Test Source");

        let retrieved = dao.get_by_id(&source.id).unwrap().unwrap();
        assert_eq!(retrieved.url, "https://example.com");

        assert_eq!(dao.get_enabled().unwrap().len(), 1);

        dao.set_enabled(&source.id, false).unwrap();
        assert!(!dao.get_by_id(&source.id).unwrap().unwrap().enabled);

        dao.delete(&source.id).unwrap();
        assert!(dao.get_by_id(&source.id).unwrap().is_none());
    }

    #[test]
    fn test_source_dao_batch_insert() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir
            .path()
            .join("test_batch.db")
            .to_string_lossy()
            .to_string();
        let mut conn = init_database(&db_path).unwrap();
        let mut dao = crate::source_dao::SourceDao::new(&mut conn);
        let now = chrono::Utc::now().timestamp();
        let sources = vec![
            crate::models::BookSource {
                id: "bs1".into(),
                name: "Source 1".into(),
                url: "https://s1.com".into(),
                source_type: 0,
                group_name: None,
                enabled: true,
                custom_order: 0,
                weight: 0,
                rule_search: None,
                rule_book_info: None,
                rule_toc: None,
                rule_content: None,
                login_url: None,
                header: None,
                js_lib: None,
                book_url_pattern: None,
                rule_explore: None,
                explore_url: None,
                enabled_explore: true,
                last_update_time: 0,
                book_source_comment: None,
                created_at: now,
                updated_at: now,
            },
            crate::models::BookSource {
                id: "bs2".into(),
                name: "Source 2".into(),
                url: "https://s2.com".into(),
                source_type: 0,
                group_name: None,
                enabled: true,
                custom_order: 0,
                weight: 0,
                rule_search: None,
                rule_book_info: None,
                rule_toc: None,
                rule_content: None,
                login_url: None,
                header: None,
                js_lib: None,
                book_url_pattern: None,
                rule_explore: None,
                explore_url: None,
                enabled_explore: true,
                last_update_time: 0,
                book_source_comment: None,
                created_at: now,
                updated_at: now,
            },
        ];
        dao.batch_insert(&sources).unwrap();
        let all = dao.get_all().unwrap();
        assert_eq!(all.len(), 2);
        assert!(all.iter().any(|s| s.name == "Source 1"));
        assert!(all.iter().any(|s| s.name == "Source 2"));
    }

    #[test]
    fn test_source_dao_import_from_json() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir
            .path()
            .join("test_import.db")
            .to_string_lossy()
            .to_string();
        let mut conn = init_database(&db_path).unwrap();
        let mut dao = crate::source_dao::SourceDao::new(&mut conn);
        let json = r#"[{"id":"ij1","name":"Imported","url":"https://imp.com","source_type":0,"enabled":true,"custom_order":0,"weight":0,"rule_search":null,"rule_book_info":null,"rule_toc":null,"rule_content":null,"login_url":null,"header":null,"js_lib":null,"created_at":0,"updated_at":0}]"#;
        let count = dao.import_from_json(json).unwrap();
        assert_eq!(count, 1);
        let all = dao.get_all().unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].name, "Imported");
    }

    #[test]
    fn test_chapter_dao_crud() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir
            .path()
            .join("test_chapter_dao.db")
            .to_string_lossy()
            .to_string();
        let conn = init_database(&db_path).unwrap();
        // Insert required book_source for foreign key constraint
        conn.execute(
            "INSERT INTO book_sources (id, name, url, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params!["src1", "Test", "https://example.com", chrono::Utc::now().timestamp(), chrono::Utc::now().timestamp()],
        ).unwrap();
        let book_dao = crate::book_dao::BookDao::new(&conn);
        let book = book_dao.create("src1", None, "Book", None).unwrap();

        let dao = crate::chapter_dao::ChapterDao::new(&conn);
        let chapter = dao
            .create(&book.id, 0, "Chapter 1", "https://example.com/ch1")
            .unwrap();
        assert_eq!(chapter.title, "Chapter 1");

        let retrieved = dao.get_by_id(&chapter.id).unwrap().unwrap();
        assert_eq!(retrieved.index_num, 0);

        dao.update_content(&chapter.id, "New content").unwrap();
        assert_eq!(
            dao.get_by_id(&chapter.id)
                .unwrap()
                .unwrap()
                .content
                .as_deref(),
            Some("New content")
        );

        assert_eq!(dao.get_by_book(&book.id).unwrap().len(), 1);

        dao.delete(&chapter.id).unwrap();
        assert!(dao.get_by_id(&chapter.id).unwrap().is_none());
    }

    #[test]
    fn test_progress_dao_crud() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir
            .path()
            .join("test_progress_dao.db")
            .to_string_lossy()
            .to_string();
        let conn = init_database(&db_path).unwrap();
        // Insert required book_source for foreign key constraint
        conn.execute(
            "INSERT INTO book_sources (id, name, url, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params!["src2", "Test", "https://example.com", chrono::Utc::now().timestamp(), chrono::Utc::now().timestamp()],
        ).unwrap();
        let book_dao = crate::book_dao::BookDao::new(&conn);
        let book = book_dao.create("src2", None, "Book", None).unwrap();

        let dao = crate::progress_dao::ProgressDao::new(&conn);
        dao.update_progress(&book.id, 3, 10, 500).unwrap();
        let progress = dao.get_by_book(&book.id).unwrap().unwrap();
        assert_eq!(progress.chapter_index, 3);
        assert_eq!(progress.paragraph_index, 10);

        let bookmark = dao.create_bookmark(&book.id, 3, 10, Some("Great")).unwrap();
        assert_eq!(dao.get_bookmarks(&book.id).unwrap().len(), 1);
        assert_eq!(
            dao.get_bookmarks(&book.id).unwrap()[0].content.as_deref(),
            Some("Great")
        );

        dao.delete_bookmark(&bookmark.id).unwrap();
        assert_eq!(dao.get_bookmarks(&book.id).unwrap().len(), 0);

        dao.delete(&book.id).unwrap();
        assert!(dao.get_by_book(&book.id).unwrap().is_none());
    }

    #[test]
    fn test_replace_rule_dao_crud() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir
            .path()
            .join("test_replace_rule_dao.db")
            .to_string_lossy()
            .to_string();
        let conn = init_database(&db_path).unwrap();
        let dao = crate::replace_rule_dao::ReplaceRuleDao::new(&conn);

        let rule = dao.create("Test Rule", r"\d+", "NUM", 0).unwrap();
        assert_eq!(rule.name, "Test Rule");
        assert!(rule.enabled);

        let retrieved = dao.get_by_id(&rule.id).unwrap().unwrap();
        assert_eq!(retrieved.pattern, r"\d+");

        assert_eq!(dao.get_all().unwrap().len(), 1);
        assert_eq!(dao.get_enabled().unwrap().len(), 1);
        assert_eq!(dao.get_by_scope(0).unwrap().len(), 1);
        assert!(dao.get_by_scope(1).unwrap().is_empty());

        dao.set_enabled(&rule.id, false).unwrap();
        assert!(!dao.get_by_id(&rule.id).unwrap().unwrap().enabled);
        assert!(dao.get_enabled().unwrap().is_empty());

        dao.update_order(&rule.id, 5).unwrap();
        assert_eq!(dao.get_by_id(&rule.id).unwrap().unwrap().sort_number, 5);

        dao.delete(&rule.id).unwrap();
        assert!(dao.get_by_id(&rule.id).unwrap().is_none());
    }
}
