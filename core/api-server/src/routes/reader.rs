use axum::{
    extract::{Json, Path, Query, State},
    routing::{get, post},
    Router,
};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;

use crate::error::ApiError;
use crate::state::AppState;
use crate::util;

#[derive(Debug, Serialize)]
pub struct ChapterContentResponse {
    pub book_id: String,
    pub chapter_index: i32,
    pub title: String,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub platform_request: Option<core_source::parser::PlatformRequest>,
}

#[derive(Debug, Deserialize)]
pub struct SaveProgressRequest {
    pub chapter_index: i32,
    pub paragraph_index: i32,
    pub offset: i32,
}

#[derive(Debug, Deserialize)]
pub struct SaveChapterContentRequest {
    pub chapter_index: i32,
    pub content: String,
}

fn hash_id(input: &str) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(input.as_bytes()))
}

#[derive(Debug, Serialize)]
pub struct RefreshChaptersResponse {
    pub book_id: String,
    pub total_count: usize,
}

async fn list_chapters(
    State(state): State<AppState>,
    Path(book_id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let conn = util::open_db(&state.db_path)?;
    let dao = core_storage::chapter_dao::ChapterDao::new(&conn);
    let chapters = dao
        .get_by_book(&book_id)
        .map_err(|e| ApiError::Database(e.to_string()))?;
    Ok(Json(serde_json::to_value(chapters)?))
}

async fn get_chapter_content(
    State(state): State<AppState>,
    Path(book_id): Path<String>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<ChapterContentResponse>, ApiError> {
    let chapter_index: i32 = params
        .get("chapter_index")
        .unwrap_or(&"0".to_string())
        .parse()
        .map_err(|_| ApiError::BadRequest("无效的章节索引".into()))?;

    let (chapter_url, chapter_title, chapter_id) = {
        let conn = util::open_db(&state.db_path)?;
        let chapter_dao = core_storage::chapter_dao::ChapterDao::new(&conn);
        let chapters = chapter_dao
            .get_by_book(&book_id)
            .map_err(|e| ApiError::Database(e.to_string()))?;

        let chapter = chapters
            .get(chapter_index as usize)
            .ok_or_else(|| {
                ApiError::NotFound(format!("章节不存在: index {}", chapter_index))
            })?;

        if let Some(ref content) = chapter.content {
            return Ok(Json(ChapterContentResponse {
                book_id,
                chapter_index,
                title: chapter.title.clone(),
                content: content.clone(),
                platform_request: None,
            }));
        }

        (chapter.url.clone(), chapter.title.clone(), chapter.id.clone())
    };

    let source_id = {
        let conn = util::open_db(&state.db_path)?;
        let dao = core_storage::book_dao::BookDao::new(&conn);
        let book = dao.get_by_id(&book_id)
            .map_err(|e| ApiError::Database(e.to_string()))?
            .ok_or_else(|| ApiError::NotFound(format!("书籍不存在: {}", book_id)))?;

        if book.source_id.is_empty() || chapter_url.is_empty() {
            return Err(ApiError::BadRequest("缺少书源信息或章节链接".into()));
        }

        book.source_id
    };

    let storage_source = {
        let mut conn = util::open_db(&state.db_path)?;
        let dao = core_storage::source_dao::SourceDao::new(&mut conn);
        dao.get_by_id(&source_id)
            .map_err(|e| ApiError::Database(e.to_string()))?
            .ok_or_else(|| ApiError::NotFound(format!("书源不存在: {}", source_id)))?
    };

    let source = util::storage_to_core_source(&storage_source)?;
    let parser = core_source::parser::BookSourceParser::new();
    let content_result = parser
        .get_chapter_content(&source, &chapter_url)
        .await;

    let (content, platform_request) = match content_result {
        Some(c) => (c.content, c.platform_request),
        None => return Err(ApiError::NotFound("章节内容为空".into())),
    };

    if platform_request.is_none() {
        let conn = util::open_db(&state.db_path)?;
        let dao = core_storage::chapter_dao::ChapterDao::new(&conn);
        dao.update_content(&chapter_id, &content)
            .map_err(|e| ApiError::Database(e.to_string()))?;
    }

    Ok(Json(ChapterContentResponse {
        book_id,
        chapter_index,
        title: chapter_title,
        content,
        platform_request,
    }))
}

async fn save_chapter_content(
    State(state): State<AppState>,
    Path(book_id): Path<String>,
    Json(req): Json<SaveChapterContentRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let chapter_id = {
        let conn = util::open_db(&state.db_path)?;
        let chapter_dao = core_storage::chapter_dao::ChapterDao::new(&conn);
        let chapters = chapter_dao
            .get_by_book(&book_id)
            .map_err(|e| ApiError::Database(e.to_string()))?;
        chapters
            .get(req.chapter_index as usize)
            .map(|chapter| chapter.id.clone())
            .ok_or_else(|| ApiError::NotFound(format!("章节不存在: index {}", req.chapter_index)))?
    };
    let conn = util::open_db(&state.db_path)?;
    let dao = core_storage::chapter_dao::ChapterDao::new(&conn);
    dao.update_content(&chapter_id, &req.content)
        .map_err(|e| ApiError::Database(e.to_string()))?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

async fn get_progress(
    State(state): State<AppState>,
    Path(book_id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let conn = util::open_db(&state.db_path)?;
    let dao = core_storage::progress_dao::ProgressDao::new(&conn);
    let progress = dao
        .get_by_book(&book_id)
        .map_err(|e| ApiError::Database(e.to_string()))?;
    Ok(Json(serde_json::to_value(progress)?))
}

async fn save_progress(
    State(state): State<AppState>,
    Path(book_id): Path<String>,
    Json(req): Json<SaveProgressRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let conn = util::open_db(&state.db_path)?;
    let book_dao = core_storage::book_dao::BookDao::new(&conn);
    book_dao
        .get_by_id(&book_id)
        .map_err(|e| ApiError::Database(e.to_string()))?
        .ok_or_else(|| ApiError::NotFound(format!("书籍不存在: {}", book_id)))?;
    let dao = core_storage::progress_dao::ProgressDao::new(&conn);
    dao.update_progress(
        &book_id,
        req.chapter_index,
        req.paragraph_index,
        req.offset,
    )
    .map_err(|e| ApiError::Database(e.to_string()))?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

async fn get_book(
    State(state): State<AppState>,
    Path(book_id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let conn = util::open_db(&state.db_path)?;
    let dao = core_storage::book_dao::BookDao::new(&conn);
    let book = dao
        .get_by_id(&book_id)
        .map_err(|e| ApiError::Database(e.to_string()))?
        .ok_or_else(|| ApiError::NotFound(format!("书籍不存在: {}", book_id)))?;
    Ok(Json(serde_json::to_value(book)?))
}

async fn refresh_chapters(
    State(state): State<AppState>,
    Path(book_id): Path<String>,
) -> Result<Json<RefreshChaptersResponse>, ApiError> {
    let (source_id, toc_url) = {
        let conn = util::open_db(&state.db_path)?;
        let dao = core_storage::book_dao::BookDao::new(&conn);
        let book = dao.get_by_id(&book_id)
            .map_err(|e| ApiError::Database(e.to_string()))?
            .ok_or_else(|| ApiError::NotFound(format!("书籍不存在: {}", book_id)))?;

        if book.source_id.is_empty() {
            return Err(ApiError::BadRequest("缺少书源信息".into()));
        }
        let url = book.toc_url
            .filter(|t| !t.trim().is_empty())
            .or_else(|| book.book_url.clone())
            .unwrap_or_default();
        if url.is_empty() {
            return Err(ApiError::BadRequest("缺少 book_url".into()));
        }
        (book.source_id, url)
    };


    let storage_source = {
        let mut conn = util::open_db(&state.db_path)?;
        let dao = core_storage::source_dao::SourceDao::new(&mut conn);
        dao.get_by_id(&source_id)
            .map_err(|e| ApiError::Database(e.to_string()))?
            .ok_or_else(|| ApiError::NotFound(format!("书源不存在: {}", source_id)))?
    };
    let source = util::storage_to_core_source(&storage_source)?;
    let parser = core_source::parser::BookSourceParser::new();
    let chapters = parser.get_chapters(&source, &toc_url).await;

    let now = chrono::Utc::now().timestamp();
    let conn = util::open_db(&state.db_path)?;
    let chapter_dao = core_storage::chapter_dao::ChapterDao::new(&conn);
    for (i, ch) in chapters.iter().enumerate() {
        let ch_id = hash_id(&format!("{}|{}|{}", book_id, ch.url, i));
        let chapter = core_storage::models::Chapter {
            id: ch_id,
            book_id: book_id.clone(),
            index_num: ch.index,
            title: ch.title.clone(),
            url: ch.url.clone(),
            content: None,
            is_volume: false,
            is_checked: false,
            start: 0,
            end: 0,
            created_at: now,
            updated_at: now,
        };
        chapter_dao
            .upsert(&chapter)
            .map_err(|e| ApiError::Database(e.to_string()))?;
    }
    let total_count = chapters.len();

    {
        let conn = util::open_db(&state.db_path)?;
        let dao = core_storage::book_dao::BookDao::new(&conn);
        let mut book = dao.get_by_id(&book_id)
            .map_err(|e| ApiError::Database(e.to_string()))?
            .ok_or_else(|| ApiError::Internal("book not found after refresh".into()))?;
        book.chapter_count = total_count as i32;
        book.updated_at = chrono::Utc::now().timestamp();
        dao.upsert(&book)
            .map_err(|e| ApiError::Database(e.to_string()))?;
    }

    Ok(Json(RefreshChaptersResponse {
        book_id,
        total_count,
    }))
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/books/:book_id", get(get_book))
        .route(
            "/api/books/:book_id/refresh-chapters",
            post(refresh_chapters),
        )
        .route("/api/books/:book_id/chapters", get(list_chapters))
        .route(
            "/api/books/:book_id/chapters/content",
            get(get_chapter_content),
        )
        .route(
            "/api/books/:book_id/chapters/content/save",
            post(save_chapter_content),
        )
        .route(
            "/api/books/:book_id/progress",
            get(get_progress).put(save_progress),
        )
}
