//! # bridge API - 暴露给 Dart 的接口
//!
//! 复杂类型使用 JSON 字符串传递，避免 FRB 类型解析问题。

// ============================================================
// 核心初始化
// ============================================================

/// Smoke test — 验证桥接是否正常工作
pub fn ping() -> String {
    "pong".to_string()
}

/// 初始化函数 - 在 Flutter 应用启动时调用
pub fn init_legado(db_path: String) -> Result<String, String> {
    core_storage::database::init_database(&db_path)
        .map(|_| "初始化成功".to_string())
        .map_err(|e| format!("初始化失败: {}", e))
}

/// 获取数据库版本
pub fn get_db_version(db_path: String) -> Result<i32, String> {
    let conn =
        core_storage::database::get_connection(&db_path).map_err(|e| format!("连接失败: {}", e))?;
    Ok(conn
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .unwrap_or(0))
}

// ============================================================
// 书架 (Books) — 返回 JSON 字符串
// ============================================================

/// 获取书架上的所有书籍，返回 JSON 数组
pub fn get_all_books(db_path: String) -> Result<String, String> {
    let conn = open_db(&db_path)?;
    let dao = core_storage::book_dao::BookDao::new(&conn);
    let books = dao
        .get_all()
        .map_err(|e| format!("获取书籍列表失败: {}", e))?;
    serde_json::to_string(&books).map_err(|e| format!("序列化失败: {}", e))
}

/// 搜索书架中的书籍，返回 JSON 数组
pub fn search_books_offline(db_path: String, keyword: String) -> Result<String, String> {
    let conn = open_db(&db_path)?;
    let dao = core_storage::book_dao::BookDao::new(&conn);
    let books = dao
        .search(&keyword)
        .map_err(|e| format!("搜索失败: {}", e))?;
    serde_json::to_string(&books).map_err(|e| format!("序列化失败: {}", e))
}

/// 保存一本书到书架（book_json 为 storage::Book 的 JSON）
pub fn save_book(db_path: String, book_json: String) -> Result<(), String> {
    let conn = open_db(&db_path)?;
    let book: core_storage::models::Book =
        serde_json::from_str(&book_json).map_err(|e| format!("JSON 解析失败: {}", e))?;
    let dao = core_storage::book_dao::BookDao::new(&conn);
    dao.upsert(&book).map_err(|e| format!("保存失败: {}", e))
}

/// 从书架删除一本书（同时级联删除该书籍的章节和阅读进度）
pub fn delete_book(db_path: String, id: String) -> Result<(), String> {
    let conn = open_db(&db_path)?;
    let book_dao = core_storage::book_dao::BookDao::new(&conn);
    // 先删除章节和进度（子记录）
    let _ = core_storage::chapter_dao::ChapterDao::new(&conn).delete_by_book(&id);
    let _ = core_storage::progress_dao::ProgressDao::new(&conn).delete(&id);
    // 再删除书籍本身
    book_dao.delete(&id).map_err(|e| format!("删除失败: {}", e))
}

// ============================================================
// 书源 (Book Sources) — 返回 JSON 字符串
// ============================================================

/// 获取所有书源，返回 JSON 数组
pub fn get_all_sources(db_path: String) -> Result<String, String> {
    let mut conn = open_db(&db_path)?;
    let dao = core_storage::source_dao::SourceDao::new(&mut conn);
    let sources = dao
        .get_all()
        .map_err(|e| format!("获取书源列表失败: {}", e))?;
    serde_json::to_string(&sources).map_err(|e| format!("序列化失败: {}", e))
}

/// 获取所有已启用的书源，返回 JSON 数组
pub fn get_enabled_sources(db_path: String) -> Result<String, String> {
    let mut conn = open_db(&db_path)?;
    let dao = core_storage::source_dao::SourceDao::new(&mut conn);
    let sources = dao
        .get_enabled()
        .map_err(|e| format!("获取已启用书源失败: {}", e))?;
    serde_json::to_string(&sources).map_err(|e| format!("序列化失败: {}", e))
}

/// 保存书源（source_json 为 storage::BookSource 的 JSON）
pub fn save_source(db_path: String, source_json: String) -> Result<(), String> {
    let mut conn = open_db(&db_path)?;
    let source: core_storage::models::BookSource =
        serde_json::from_str(&source_json).map_err(|e| format!("JSON 解析失败: {}", e))?;
    let dao = core_storage::source_dao::SourceDao::new(&mut conn);
    dao.upsert(&source)
        .map(|_| ())
        .map_err(|e| format!("保存书源失败: {}", e))
}

/// 便捷函数：仅需 name + url 即可创建书源（自动填充 id/enabled/timestamps 等必填字段）
pub fn create_source(db_path: String, name: String, url: String) -> Result<String, String> {
    let mut conn = open_db(&db_path)?;
    let dao = core_storage::source_dao::SourceDao::new(&mut conn);
    let source = dao
        .create(&name, &url)
        .map_err(|e| format!("创建书源失败: {}", e))?;
    serde_json::to_string(&source).map_err(|e| format!("序列化失败: {}", e))
}

/// 删除书源
pub fn delete_source(db_path: String, id: String) -> Result<(), String> {
    let mut conn = open_db(&db_path)?;
    let dao = core_storage::source_dao::SourceDao::new(&mut conn);
    dao.delete(&id).map_err(|e| format!("删除书源失败: {}", e))
}

/// 批量删除书源 (ids_json 为 JSON 字符串数组)
pub fn delete_sources_batch(db_path: String, ids_json: String) -> Result<(), String> {
    let mut conn = open_db(&db_path)?;
    let ids: Vec<String> =
        serde_json::from_str(&ids_json).map_err(|e| format!("JSON 解析失败: {}", e))?;
    let dao = core_storage::source_dao::SourceDao::new(&mut conn);
    dao.delete_batch(&ids)
        .map_err(|e| format!("批量删除书源失败: {}", e))
}

/// 启用 / 禁用书源
pub fn set_source_enabled(db_path: String, id: String, enabled: bool) -> Result<(), String> {
    let mut conn = open_db(&db_path)?;
    let dao = core_storage::source_dao::SourceDao::new(&mut conn);
    dao.set_enabled(&id, enabled)
        .map_err(|e| format!("更新书源状态失败: {}", e))
}

/// 从 JSON 批量导入书源
pub fn import_sources_from_json(db_path: String, json: String) -> Result<i32, String> {
    let mut conn = open_db(&db_path)?;
    let mut dao = core_storage::source_dao::SourceDao::new(&mut conn);
    let count = dao
        .import_from_json(&json)
        .map_err(|e| format!("导入书源失败: {}", e))?;
    Ok(count as i32)
}

/// 获取书源（core_source::types::BookSource 格式），用于下载
pub fn get_source_for_download(db_path: String, source_id: String) -> Result<String, String> {
    let storage_source = {
        let mut conn = open_db(&db_path)?;
        let dao = core_storage::source_dao::SourceDao::new(&mut conn);
        dao.get_by_id(&source_id)
            .map_err(|e| format!("查询书源失败: {}", e))?
            .ok_or_else(|| format!("书源不存在: {}", source_id))?
    };
    let source = storage_to_source_book_source(&storage_source)?;
    serde_json::to_string(&source).map_err(|e| format!("序列化失败: {}", e))
}

// ============================================================
// 章节 (Chapters) — 返回 JSON 字符串
// ============================================================

/// 获取某本书的所有章节，返回 JSON 数组
pub fn get_book_chapters(db_path: String, book_id: String) -> Result<String, String> {
    let conn = open_db(&db_path)?;
    let dao = core_storage::chapter_dao::ChapterDao::new(&conn);
    let chapters = dao
        .get_by_book(&book_id)
        .map_err(|e| format!("获取章节列表失败: {}", e))?;
    serde_json::to_string(&chapters).map_err(|e| format!("序列化失败: {}", e))
}

/// 更新章节内容
pub fn update_chapter_content(
    db_path: String,
    chapter_id: String,
    content: String,
) -> Result<(), String> {
    let conn = open_db(&db_path)?;
    let dao = core_storage::chapter_dao::ChapterDao::new(&conn);
    dao.update_content(&chapter_id, &content)
        .map_err(|e| format!("更新章节内容失败: {}", e))
}

/// 保存章节（chapter_json 为 storage::Chapter 的 JSON）
pub fn save_chapter(db_path: String, chapter_json: String) -> Result<(), String> {
    let conn = open_db(&db_path)?;
    let chapter: core_storage::models::Chapter =
        serde_json::from_str(&chapter_json).map_err(|e| format!("JSON 解析失败: {}", e))?;
    let dao = core_storage::chapter_dao::ChapterDao::new(&conn);
    dao.upsert(&chapter)
        .map_err(|e| format!("保存章节失败: {}", e))
}

/// 批量替换某本书的章节（chapters_json 为 storage::Chapter 数组 JSON），保留相同 URL 的已缓存正文
pub fn replace_book_chapters_preserving_content(
    db_path: String,
    book_id: String,
    chapters_json: String,
) -> Result<(), String> {
    let conn = open_db(&db_path)?;
    let chapters: Vec<core_storage::models::Chapter> =
        serde_json::from_str(&chapters_json).map_err(|e| format!("JSON 解析失败: {}", e))?;
    let dao = core_storage::chapter_dao::ChapterDao::new(&conn);
    dao.replace_by_book_preserving_content(&book_id, &chapters)
        .map_err(|e| format!("批量保存章节失败: {}", e))
}

/// 批量替换某本书的章节（chapters_json 为 storage::Chapter 数组 JSON），不保留旧章节正文
pub fn replace_book_chapters(
    db_path: String,
    book_id: String,
    chapters_json: String,
) -> Result<(), String> {
    let conn = open_db(&db_path)?;
    let chapters: Vec<core_storage::models::Chapter> =
        serde_json::from_str(&chapters_json).map_err(|e| format!("JSON 解析失败: {}", e))?;
    let dao = core_storage::chapter_dao::ChapterDao::new(&conn);
    dao.replace_by_book(&book_id, &chapters)
        .map_err(|e| format!("批量替换章节失败: {}", e))
}

/// 删除章节
pub fn delete_chapter(db_path: String, id: String) -> Result<(), String> {
    let conn = open_db(&db_path)?;
    let dao = core_storage::chapter_dao::ChapterDao::new(&conn);
    dao.delete(&id).map_err(|e| format!("删除章节失败: {}", e))
}

// ============================================================
// 阅读进度 (Reading Progress)
// ============================================================

/// 保存阅读进度
pub fn save_reading_progress(
    db_path: String,
    book_id: String,
    chapter_index: i32,
    paragraph_index: i32,
    offset: i32,
) -> Result<(), String> {
    let conn = open_db(&db_path)?;
    let dao = core_storage::progress_dao::ProgressDao::new(&conn);
    dao.update_progress(&book_id, chapter_index, paragraph_index, offset)
        .map_err(|e| format!("保存阅读进度失败: {}", e))
}

/// 获取阅读进度，返回 JSON 或 null
pub fn get_reading_progress(db_path: String, book_id: String) -> Result<String, String> {
    let conn = open_db(&db_path)?;
    let dao = core_storage::progress_dao::ProgressDao::new(&conn);
    let progress = dao
        .get_by_book(&book_id)
        .map_err(|e| format!("获取进度失败: {}", e))?;
    serde_json::to_string(&progress).map_err(|e| format!("序列化失败: {}", e))
}

// ============================================================
// 书签 (Bookmarks) — 返回 JSON 字符串
// ============================================================

/// 获取某本书的所有书签，返回 JSON 数组
pub fn get_bookmarks(db_path: String, book_id: String) -> Result<String, String> {
    let conn = open_db(&db_path)?;
    let dao = core_storage::progress_dao::ProgressDao::new(&conn);
    let bookmarks = dao
        .get_bookmarks(&book_id)
        .map_err(|e| format!("获取书签失败: {}", e))?;
    serde_json::to_string(&bookmarks).map_err(|e| format!("序列化失败: {}", e))
}

/// 添加书签，返回新书签的 JSON
pub fn add_bookmark(
    db_path: String,
    book_id: String,
    chapter_index: i32,
    paragraph_index: i32,
    content: Option<String>,
) -> Result<String, String> {
    let conn = open_db(&db_path)?;
    let dao = core_storage::progress_dao::ProgressDao::new(&conn);
    let bookmark = dao
        .create_bookmark(&book_id, chapter_index, paragraph_index, content.as_deref())
        .map_err(|e| format!("添加书签失败: {}", e))?;
    serde_json::to_string(&bookmark).map_err(|e| format!("序列化失败: {}", e))
}

/// 删除书签
pub fn delete_bookmark(db_path: String, bookmark_id: String) -> Result<(), String> {
    let conn = open_db(&db_path)?;
    let dao = core_storage::progress_dao::ProgressDao::new(&conn);
    dao.delete_bookmark(&bookmark_id)
        .map_err(|e| format!("删除书签失败: {}", e))
}

// ============================================================
// 在线搜索 & 内容获取 — 返回 JSON 字符串
// ============================================================

/// 搜索在线书籍（source_json 为 core_source::BookSource 的 JSON），返回搜索结果 JSON 数组
pub async fn search_books_online(source_json: String, keyword: String) -> Result<String, String> {
    let source: core_source::types::BookSource =
        serde_json::from_str(&source_json).map_err(|e| format!("解析书源失败: {}", e))?;
    let parser = core_source::parser::BookSourceParser::new();
    let results = parser.search(&source, &keyword).await;
    serde_json::to_string(&results).map_err(|e| format!("序列化失败: {}", e))
}

/// 获取在线书籍详情，返回 JSON 或 null
pub async fn get_book_info_online(source_json: String, book_url: String) -> Result<String, String> {
    let source: core_source::types::BookSource =
        serde_json::from_str(&source_json).map_err(|e| format!("解析书源失败: {}", e))?;
    let parser = core_source::parser::BookSourceParser::new();
    let detail = parser.get_book_info(&source, &book_url).await;
    serde_json::to_string(&detail).map_err(|e| format!("序列化失败: {}", e))
}

/// 获取在线章节列表，返回 JSON 数组
pub async fn get_chapter_list_online(
    source_json: String,
    book_url: String,
) -> Result<String, String> {
    let source: core_source::types::BookSource =
        serde_json::from_str(&source_json).map_err(|e| format!("解析书源失败: {}", e))?;
    let parser = core_source::parser::BookSourceParser::new();
    let chapters = parser.get_chapters(&source, &book_url).await;
    serde_json::to_string(&chapters).map_err(|e| format!("序列化失败: {}", e))
}

/// 获取在线章节内容，返回 JSON 或 null
pub async fn get_chapter_content_online(
    source_json: String,
    chapter_url: String,
) -> Result<String, String> {
    let source: core_source::types::BookSource =
        serde_json::from_str(&source_json).map_err(|e| format!("解析书源失败: {}", e))?;
    let parser = core_source::parser::BookSourceParser::new();
    let content = parser.get_chapter_content(&source, &chapter_url).await;
    serde_json::to_string(&content).map_err(|e| format!("序列化失败: {}", e))
}

// ============================================================
// 诊断函数：导出书源原始规则用于调试
// ============================================================

/// 获取书源的原始 rule_search JSON（用于诊断），返回 JSON 或 null
pub fn get_source_rule_search_raw(
    db_path: String,
    source_id: String,
) -> Result<String, String> {
    let mut conn = open_db(&db_path)?;
    let dao = core_storage::source_dao::SourceDao::new(&mut conn);
    let source = dao
        .get_by_id(&source_id)
        .map_err(|e| format!("查询书源失败: {}", e))?
        .ok_or_else(|| format!("书源不存在: {}", source_id))?;
    serde_json::to_string(&source.rule_search).map_err(|e| format!("序列化失败: {}", e))
}

// ============================================================
// 便捷函数：从数据库取书源后直接操作
// ============================================================

/// 从数据库加载书源并搜索（异步），返回搜索结果 JSON 数组
/// 包装结果：正常时返回 [{"ok":true,"data":[...]}]，失败时返回 [{"ok":false,"error":"..."}]
pub async fn search_with_source_from_db_v2(
    db_path: String,
    source_id: String,
    keyword: String,
) -> Result<String, String> {
    let storage_source = {
        let mut conn = open_db(&db_path)?;
        let dao = core_storage::source_dao::SourceDao::new(&mut conn);
        dao.get_by_id(&source_id)
            .map_err(|e| format!("查询书源失败: {}", e))?
            .ok_or_else(|| format!("书源不存在: {}", source_id))?
    };

    let source = match storage_to_source_book_source(&storage_source) {
        Ok(s) => s,
        Err(e) => {
            let resp = serde_json::json!({"ok": false, "error": format!("转换书源失败: {}", e), "source_name": storage_source.name});
            return serde_json::to_string(&vec![resp]).map_err(|e| format!("序列化失败: {}", e));
        }
    };

    let parser = core_source::parser::BookSourceParser::new();
    match parser.search(&source, &keyword).await {
        results if results.is_empty() => {
            let resp = serde_json::json!({"ok": false, "error": "搜索返回0结果", "source_name": source.name, "search_url": source.rule_search.as_ref().and_then(|r| r.search_url.as_ref().cloned()).unwrap_or_default()});
            serde_json::to_string(&vec![resp])
                .map_err(|e| format!("序列化失败: {}", e))
        }
        results => {
            serde_json::to_string(&results).map_err(|e| format!("序列化失败: {}", e))
        }
    }
}

/// 使用预获取的 HTML 搜索（由 Dart 端 dio 提供 HTML，本端仅解析）。
/// 返回搜索结果 JSON 数组。sync 函数，不含 HTTP 请求。
pub fn search_parse_html(
    db_path: String,
    source_id: String,
    keyword: String,
    html: String,
) -> Result<String, String> {
    let storage_source = {
        let mut conn = open_db(&db_path)?;
        let dao = core_storage::source_dao::SourceDao::new(&mut conn);
        dao.get_by_id(&source_id)
            .map_err(|e| format!("查询书源失败: {}", e))?
            .ok_or_else(|| format!("书源不存在: {}", source_id))?
    };
    let source = storage_to_source_book_source(&storage_source)?;
    let parser = core_source::parser::BookSourceParser::new();

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| format!("创建 tokio runtime 失败: {}", e))?;
    let results = rt.block_on(async { parser.search_html(&source, &keyword, &html).await });
    drop(rt);
    serde_json::to_string(&results).map_err(|e| format!("序列化失败: {}", e))
}

/// 从数据库加载书源并搜索（异步），返回搜索结果 JSON 数组
pub async fn search_with_source_from_db(
    db_path: String,
    source_id: String,
    keyword: String,
) -> Result<String, String> {
    let storage_source = {
        let mut conn = open_db(&db_path)?;
        let dao = core_storage::source_dao::SourceDao::new(&mut conn);
        dao.get_by_id(&source_id)
            .map_err(|e| format!("查询书源失败: {}", e))?
            .ok_or_else(|| format!("书源不存在: {}", source_id))?
    };

    let source = storage_to_source_book_source(&storage_source)?;
    let parser = core_source::parser::BookSourceParser::new();
    let results = parser.search(&source, &keyword).await;
    serde_json::to_string(&results).map_err(|e| format!("序列化失败: {}", e))
}

/// 从数据库加载书源并获取章节内容（异步），返回 JSON 或 null
pub async fn get_chapter_content_with_source_from_db(
    db_path: String,
    source_id: String,
    chapter_url: String,
) -> Result<String, String> {
    let storage_source = {
        let mut conn = open_db(&db_path)?;
        let dao = core_storage::source_dao::SourceDao::new(&mut conn);
        dao.get_by_id(&source_id)
            .map_err(|e| format!("查询书源失败: {}", e))?
            .ok_or_else(|| format!("书源不存在: {}", source_id))?
    };

    let source = storage_to_source_book_source(&storage_source)?;
    let parser = core_source::parser::BookSourceParser::new();
    let content = parser.get_chapter_content(&source, &chapter_url).await;
    serde_json::to_string(&content).map_err(|e| format!("序列化失败: {}", e))
}

// ============================================================
// 下载管理 (Download Management)
// ============================================================

/// 获取所有下载任务，返回 JSON 数组
pub fn get_download_tasks(db_path: String) -> Result<String, String> {
    let conn = open_db(&db_path)?;
    let dao = core_storage::download_dao::DownloadDao::new(&conn);
    let tasks = dao
        .get_all()
        .map_err(|e| format!("获取下载任务失败: {}", e))?;
    serde_json::to_string(&tasks).map_err(|e| format!("序列化失败: {}", e))
}

/// 根据书籍 ID 获取下载任务
pub fn get_download_task_by_book(db_path: String, book_id: String) -> Result<String, String> {
    let conn = open_db(&db_path)?;
    let dao = core_storage::download_dao::DownloadDao::new(&conn);
    let tasks = dao
        .get_by_book(&book_id)
        .map_err(|e| format!("获取下载任务失败: {}", e))?;
    serde_json::to_string(&tasks).map_err(|e| format!("序列化失败: {}", e))
}

/// 创建下载任务（task_json 为 DownloadTask 的 JSON），返回 JSON
pub fn create_download_task(db_path: String, task_json: String) -> Result<String, String> {
    let conn = open_db(&db_path)?;
    let task: core_storage::models::DownloadTask =
        serde_json::from_str(&task_json).map_err(|e| format!("JSON 解析失败: {}", e))?;
    let dao = core_storage::download_dao::DownloadDao::new(&conn);
    dao.upsert(&task)
        .map_err(|e| format!("创建下载任务失败: {}", e))?;
    serde_json::to_string(&task).map_err(|e| format!("序列化失败: {}", e))
}

/// 事务性创建下载任务和章节
pub fn create_download_task_with_chapters(
    db_path: String,
    task_json: String,
    chapters_json: String,
) -> Result<String, String> {
    let conn = open_db(&db_path)?;
    let task: core_storage::models::DownloadTask =
        serde_json::from_str(&task_json).map_err(|e| format!("JSON 解析失败: {}", e))?;
    let chapters: Vec<core_storage::models::DownloadChapter> =
        serde_json::from_str(&chapters_json).map_err(|e| format!("JSON 解析失败: {}", e))?;
    let dao = core_storage::download_dao::DownloadDao::new(&conn);
    dao.create_task_with_chapters(&task, &chapters)
        .map_err(|e| format!("创建下载任务失败: {}", e))?;
    serde_json::to_string(&task).map_err(|e| format!("序列化失败: {}", e))
}

/// 更新下载任务状态
pub fn update_download_task_status(
    db_path: String,
    task_id: String,
    status: i32,
    error_message: Option<String>,
) -> Result<(), String> {
    let conn = open_db(&db_path)?;
    let dao = core_storage::download_dao::DownloadDao::new(&conn);
    dao.update_status(&task_id, status, error_message.as_deref())
        .map_err(|e| format!("更新下载状态失败: {}", e))
}

/// 更新下载进度
pub fn update_download_progress(
    db_path: String,
    task_id: String,
    downloaded_chapters: i32,
    downloaded_size: i64,
) -> Result<(), String> {
    let conn = open_db(&db_path)?;
    let dao = core_storage::download_dao::DownloadDao::new(&conn);
    dao.update_progress(&task_id, downloaded_chapters, downloaded_size)
        .map_err(|e| format!("更新下载进度失败: {}", e))
}

/// 删除下载任务（同时清理已下载的文件）
pub fn delete_download_task(db_path: String, task_id: String) -> Result<(), String> {
    let conn = open_db(&db_path)?;
    let dao = core_storage::download_dao::DownloadDao::new(&conn);
    let inferred_root = std::path::Path::new(&db_path)
        .parent()
        .map(|parent| parent.join("downloads"));
    dao.delete_with_files_in_root(&task_id, inferred_root.as_deref())
        .map_err(|e| format!("删除下载任务失败: {}", e))
}

/// 获取下载章节列表，返回 JSON 数组
pub fn get_download_chapters(db_path: String, task_id: String) -> Result<String, String> {
    let conn = open_db(&db_path)?;
    let dao = core_storage::download_dao::DownloadDao::new(&conn);
    let chapters = dao
        .get_chapters_by_task(&task_id)
        .map_err(|e| format!("获取下载章节失败: {}", e))?;
    serde_json::to_string(&chapters).map_err(|e| format!("序列化失败: {}", e))
}

/// 批量创建下载章节记录
pub fn batch_create_download_chapters(
    db_path: String,
    chapters_json: String,
) -> Result<(), String> {
    let conn = open_db(&db_path)?;
    let chapters: Vec<core_storage::models::DownloadChapter> =
        serde_json::from_str(&chapters_json).map_err(|e| format!("JSON 解析失败: {}", e))?;
    let dao = core_storage::download_dao::DownloadDao::new(&conn);
    dao.batch_create_chapters(&chapters)
        .map_err(|e| format!("批量创建下载章节失败: {}", e))
}

/// 更新下载章节状态
pub fn update_download_chapter_status(
    db_path: String,
    chapter_id: String,
    status: i32,
    file_path: Option<String>,
    file_size: i64,
    error_message: Option<String>,
) -> Result<(), String> {
    let conn = open_db(&db_path)?;
    let dao = core_storage::download_dao::DownloadDao::new(&conn);
    dao.update_chapter_status(
        &chapter_id,
        status,
        file_path.as_deref(),
        file_size,
        error_message.as_deref(),
    )
    .map_err(|e| format!("更新下载章节状态失败: {}", e))
}

/// 下载并保存单个章节到本地文件，更新数据库状态
pub async fn download_and_save_chapter(
    db_path: String,
    task_id: String,
    download_chapter_id: String,
    source_json: String,
    chapter_url: String,
    download_dir: String,
) -> Result<String, String> {
    let download_root = resolve_download_root(&db_path, &download_dir)?;
    core_storage::download_dao::set_download_root(&download_root.to_string_lossy());
    let source: core_source::types::BookSource =
        serde_json::from_str(&source_json).map_err(|e| format!("解析书源失败: {}", e))?;
    let parser = core_source::parser::BookSourceParser::new();
    let content = parser.get_chapter_content(&source, &chapter_url).await;

    let text = match &content {
        Some(c) => c.content.clone(),
        None => {
            let conn = open_db(&db_path)?;
            let dao = core_storage::download_dao::DownloadDao::new(&conn);
            dao.update_chapter_status(&download_chapter_id, 3, None, 0, Some("章节内容为空"))
                .map_err(|e| format!("更新章节状态失败: {}", e))?;
            recompute_download_task_status(&dao, &task_id)
                .map_err(|e| format!("更新任务状态失败: {}", e))?;
            return Err("章节内容为空".to_string());
        }
    };

    let file_name = safe_download_file_name(&download_chapter_id)?;
    let file_path = download_root.join(file_name);
    let parent = file_path
        .parent()
        .ok_or_else(|| "下载文件路径无效".to_string())?;
    std::fs::create_dir_all(parent).map_err(|e| format!("创建目录失败: {}", e))?;
    let root_canonical = download_root
        .canonicalize()
        .map_err(|e| format!("下载目录无效: {}", e))?;
    let parent_canonical = parent
        .canonicalize()
        .map_err(|e| format!("下载目录无效: {}", e))?;
    if !parent_canonical.starts_with(&root_canonical) {
        return Err("下载文件路径越界".to_string());
    }
    if std::fs::symlink_metadata(&file_path)
        .map(|metadata| metadata.file_type().is_symlink())
        .unwrap_or(false)
    {
        return Err("下载文件不能是符号链接".to_string());
    }
    std::fs::write(&file_path, &text).map_err(|e| format!("写入文件失败: {}", e))?;
    let file_size = std::fs::metadata(&file_path)
        .map(|m| m.len() as i64)
        .unwrap_or(0);
    let file_path = file_path.to_string_lossy().to_string();

    let conn = open_db(&db_path)?;
    let dao = core_storage::download_dao::DownloadDao::new(&conn);
    dao.update_chapter_status(&download_chapter_id, 2, Some(&file_path), file_size, None)
        .map_err(|e| format!("更新章节状态失败: {}", e))?;
    recompute_download_task_status(&dao, &task_id)
        .map_err(|e| format!("更新任务状态失败: {}", e))?;

    Ok(file_path)
}

fn recompute_download_task_status(
    dao: &core_storage::download_dao::DownloadDao<'_>,
    task_id: &str,
) -> rusqlite::Result<()> {
    let chapters = dao.get_chapters_by_task(task_id)?;
    let completed_count = chapters.iter().filter(|c| c.status == 2).count() as i32;
    let failed_count = chapters.iter().filter(|c| c.status == 3).count() as i32;
    let total_count = chapters.len() as i32;
    let total_size: i64 = chapters.iter().map(|c| c.file_size).sum();

    dao.update_progress(task_id, completed_count, total_size)?;

    if completed_count + failed_count >= total_count {
        if failed_count > 0 {
            dao.update_status(
                task_id,
                4,
                Some(&format!(
                    "部分章节下载失败 (成功: {}, 失败: {})",
                    completed_count, failed_count
                )),
            )?;
        } else {
            dao.update_status(task_id, 3, None)?;
        }
    }
    Ok(())
}

fn resolve_download_root(db_path: &str, download_dir: &str) -> Result<std::path::PathBuf, String> {
    let expected = std::path::Path::new(db_path)
        .parent()
        .ok_or_else(|| "数据库路径无效".to_string())?
        .join("downloads");
    std::fs::create_dir_all(&expected).map_err(|e| format!("创建下载目录失败: {}", e))?;
    let expected_canonical = expected
        .canonicalize()
        .map_err(|e| format!("下载目录无效: {}", e))?;
    let requested = std::path::Path::new(download_dir);
    let requested_canonical = requested
        .canonicalize()
        .map_err(|e| format!("下载目录无效: {}", e))?;
    if requested_canonical != expected_canonical {
        return Err("下载目录必须位于数据库目录下的 downloads".to_string());
    }
    Ok(expected_canonical)
}

fn safe_download_file_name(download_chapter_id: &str) -> Result<String, String> {
    let id = download_chapter_id.trim();
    if id.is_empty()
        || id == "."
        || id == ".."
        || id.contains('/')
        || id.contains('\\')
        || id.contains(std::path::MAIN_SEPARATOR)
    {
        return Err("下载章节 ID 不能作为安全文件名".to_string());
    }
    Ok(format!("{id}.txt"))
}

/// 验证书源规则（source_json 为 core_source::types::BookSource 的 JSON），返回 JSON 数组
pub fn validate_source_rules(source_json: String) -> Result<String, String> {
    core_source::validate_source_json(&source_json).map_err(|e| format!("验证书源规则失败: {}", e))
}

/// 验证数据库中的书源规则，返回 JSON 数组 [{field, severity, message}]
pub fn validate_source_from_db(db_path: String, source_id: String) -> Result<String, String> {
    let storage_source = {
        let mut conn = open_db(&db_path)?;
        let dao = core_storage::source_dao::SourceDao::new(&mut conn);
        dao.get_by_id(&source_id)
            .map_err(|e| format!("查询书源失败: {}", e))?
            .ok_or_else(|| format!("书源不存在: {}", source_id))?
    };
    let source = storage_to_source_book_source(&storage_source)?;
    let issues = core_source::validate_book_source(&source);
    serde_json::to_string(&issues).map_err(|e| format!("序列化失败: {}", e))
}

/// 导出所有书源为 Legado 兼容 JSON 数组（camelCase 格式）
pub fn export_all_sources(db_path: String) -> Result<String, String> {
    let mut conn = open_db(&db_path)?;
    let dao = core_storage::source_dao::SourceDao::new(&mut conn);
    dao.export_legado_json()
        .map_err(|e| format!("导出失败: {}", e))
}

// ============================================================
// 发现页 (Explore) — FRB 桥接
// ============================================================

/// 获取书源的发现入口列表
pub fn get_explore_entries(db_path: String, source_id: String) -> Result<String, String> {
    let mut conn = open_db(&db_path)?;
    let dao = core_storage::source_dao::SourceDao::new(&mut conn);
    let source = dao
        .get_by_id(&source_id)
        .map_err(|e| format!("数据库错误: {}", e))?
        .ok_or_else(|| format!("书源不存在: {}", source_id))?;
    let core_source = storage_to_source_book_source(&source)?;
    let entries = core_source::parser::BookSourceParser::get_explore_entries(&core_source);
    serde_json::to_string(&entries).map_err(|e| format!("序列化失败: {}", e))
}

fn block_on_explore<F, R>(f: F) -> R
where
    F: FnOnce(&tokio::runtime::Runtime) -> R,
{
    static RT: std::sync::OnceLock<tokio::runtime::Runtime> = std::sync::OnceLock::new();
    let rt = RT.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .enable_all()
            .build()
            .expect("explore runtime")
    });
    f(rt)
}

/// 执行发现页请求，获取书籍列表
pub fn explore(
    db_path: String,
    source_id: String,
    explore_url: String,
    page: i32,
) -> Result<String, String> {
    let mut conn = open_db(&db_path)?;
    let dao = core_storage::source_dao::SourceDao::new(&mut conn);
    let source = dao
        .get_by_id(&source_id)
        .map_err(|e| format!("数据库错误: {}", e))?
        .ok_or_else(|| format!("书源不存在: {}", source_id))?;
    let core_source = storage_to_source_book_source(&source)?;
    let parser = core_source::parser::BookSourceParser::new();
    let results =
        block_on_explore(|rt| rt.block_on(parser.explore(&core_source, &explore_url, page)));
    serde_json::to_string(&results).map_err(|e| format!("序列化失败: {}", e))
}

// ============================================================
// 替换规则 (Replace Rules) — 返回 JSON 字符串
// ============================================================

/// 获取所有替换规则，返回 JSON 数组
pub fn get_replace_rules(db_path: String) -> Result<String, String> {
    let mut conn = open_db(&db_path)?;
    let dao = core_storage::replace_rule_dao::ReplaceRuleDao::new(&mut conn);
    let rules = dao
        .get_all()
        .map_err(|e| format!("获取替换规则列表失败: {}", e))?;
    serde_json::to_string(&rules).map_err(|e| format!("序列化失败: {}", e))
}

/// 保存替换规则（rule_json 为 storage::ReplaceRule 的 JSON），upsert 语义
pub fn save_replace_rule(db_path: String, rule_json: String) -> Result<(), String> {
    let mut conn = open_db(&db_path)?;
    let rule: core_storage::models::ReplaceRule =
        serde_json::from_str(&rule_json).map_err(|e| format!("JSON 解析失败: {}", e))?;
    let dao = core_storage::replace_rule_dao::ReplaceRuleDao::new(&mut conn);
    dao.upsert(&rule)
        .map_err(|e| format!("保存替换规则失败: {}", e))
}

/// 删除替换规则
pub fn delete_replace_rule(db_path: String, id: String) -> Result<(), String> {
    let mut conn = open_db(&db_path)?;
    let dao = core_storage::replace_rule_dao::ReplaceRuleDao::new(&mut conn);
    dao.delete(&id)
        .map_err(|e| format!("删除替换规则失败: {}", e))
}

/// 启用 / 禁用替换规则
pub fn set_replace_rule_enabled(db_path: String, id: String, enabled: bool) -> Result<(), String> {
    let mut conn = open_db(&db_path)?;
    let dao = core_storage::replace_rule_dao::ReplaceRuleDao::new(&mut conn);
    dao.set_enabled(&id, enabled)
        .map_err(|e| format!("更新替换规则状态失败: {}", e))
}

// ============================================================
// 内部辅助函数
// ============================================================

fn open_db(db_path: &str) -> Result<rusqlite::Connection, String> {
    core_storage::database::get_connection(db_path).map_err(|e| format!("数据库连接失败: {}", e))
}

fn storage_to_source_book_source(
    s: &core_storage::models::BookSource,
) -> Result<core_source::types::BookSource, String> {
    let rule_search = s
        .rule_search
        .as_deref()
        .map(|r| serde_json::from_str(r).map_err(|e| format!("解析 rule_search JSON 失败: {}", e)))
        .transpose()?;

    let rule_book_info = s
        .rule_book_info
        .as_deref()
        .map(|r| {
            serde_json::from_str(r).map_err(|e| format!("解析 rule_book_info JSON 失败: {}", e))
        })
        .transpose()?;

    let rule_toc = s
        .rule_toc
        .as_deref()
        .map(|r| serde_json::from_str(r).map_err(|e| format!("解析 rule_toc JSON 失败: {}", e)))
        .transpose()?;

    let rule_content = s
        .rule_content
        .as_deref()
        .map(|r| serde_json::from_str(r).map_err(|e| format!("解析 rule_content JSON 失败: {}", e)))
        .transpose()?;

    let rule_explore = s
        .rule_explore
        .as_deref()
        .map(|r| serde_json::from_str(r).map_err(|e| format!("解析 rule_explore JSON 失败: {}", e)))
        .transpose()?;

    Ok(core_source::types::BookSource {
        id: s.id.clone(),
        name: s.name.clone(),
        url: s.url.clone(),
        source_type: s.source_type,
        enabled: s.enabled,
        group_name: s.group_name.clone(),
        custom_order: s.custom_order,
        weight: s.weight,
        rule_search,
        rule_book_info,
        rule_toc,
        rule_content,
        login_url: s.login_url.clone(),
        header: s.header.clone(),
        js_lib: s.js_lib.clone(),
        explore_url: s.explore_url.clone(),
        rule_explore,
        book_url_pattern: s.book_url_pattern.clone(),
        enabled_explore: s.enabled_explore,
        last_update_time: s.last_update_time,
        book_source_comment: s.book_source_comment.clone(),
        created_at: s.created_at,
        updated_at: s.updated_at,
    })
}
