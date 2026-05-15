use axum::{
    extract::{Json, State},
    routing::post,
    Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::Semaphore;
use tokio::task::JoinSet;

use crate::error::ApiError;
use crate::state::AppState;
use crate::util;

#[derive(Debug, Deserialize)]
pub struct SearchRequest {
    pub keyword: String,
    pub source_ids: Option<Vec<String>>,
}

#[derive(Debug, Serialize)]
pub struct SearchResponse {
    pub items: Vec<core_source::parser::SearchResult>,
    pub failed_sources: Vec<FailedSource>,
}

#[derive(Debug, Serialize)]
pub struct FailedSource {
    pub source_id: String,
    pub source_name: String,
    pub error: String,
}

async fn search(
    State(state): State<AppState>,
    Json(req): Json<SearchRequest>,
) -> Result<Json<SearchResponse>, ApiError> {
    if req.keyword.trim().is_empty() {
        return Err(ApiError::BadRequest("搜索关键词不能为空".into()));
    }

    let source_ids: Vec<String> = if let Some(ids) = req.source_ids {
        if ids.is_empty() {
            return Ok(Json(SearchResponse {
                items: vec![],
                failed_sources: vec![],
            }));
        }
        ids
    } else {
        let mut conn = util::open_db(&state.db_path)?;
        let dao = core_storage::source_dao::SourceDao::new(&mut conn);
        dao.get_enabled()
            .map_err(|e| ApiError::Database(e.to_string()))?
            .into_iter()
            .map(|s| s.id)
            .collect()
    };

    let semaphore = Arc::new(Semaphore::new(16));
    let mut join_set = JoinSet::new();
    for sid in &source_ids {
        let sid = sid.clone();
        let db_path = state.db_path.clone();
        let keyword = req.keyword.clone();
        let permit = semaphore
            .clone()
            .acquire_owned()
            .await
            .map_err(|_| ApiError::Internal("信号量获取失败".into()))?;
        join_set.spawn(async move {
            let result = search_single_source(&db_path, &sid, &keyword).await;
            drop(permit);
            result
        });
    }

    let mut items = Vec::new();
    let mut failed_sources = Vec::new();
    while let Some(result) = join_set.join_next().await {
        match result {
            Ok(Ok(search_results)) => items.extend(search_results),
            Ok(Err((id, name, error))) => {
                failed_sources.push(FailedSource {
                    source_id: id,
                    source_name: if name.is_empty() {
                        "未知书源".into()
                    } else {
                        name
                    },
                    error,
                });
            }
            Err(e) => {
                failed_sources.push(FailedSource {
                    source_id: "unknown".into(),
                    source_name: "未知书源".into(),
                    error: format!("任务执行失败: {}", e),
                });
            }
        }
    }

    Ok(Json(SearchResponse {
        items,
        failed_sources,
    }))
}

async fn search_single_source(
    db_path: &str,
    source_id: &str,
    keyword: &str,
) -> Result<Vec<core_source::parser::SearchResult>, (String, String, String)> {
    let storage_source = {
        let mut conn = util::open_db(db_path)
            .map_err(|e| (source_id.to_string(), "".into(), e.to_string()))?;
        let dao = core_storage::source_dao::SourceDao::new(&mut conn);
        dao.get_by_id(source_id)
            .map_err(|e| (source_id.to_string(), "".into(), e.to_string()))?
            .ok_or_else(|| (source_id.to_string(), "".into(), "书源不存在".into()))?
    };
    let source_name = storage_source.name.clone();
    let source = util::storage_to_core_source(&storage_source)
        .map_err(|e| (source_id.to_string(), source_name.clone(), e.to_string()))?;
    let parser = core_source::parser::BookSourceParser::new();
    let results = parser.search(&source, keyword).await;
    Ok(results)
}

pub fn routes() -> Router<AppState> {
    Router::new().route("/api/search", post(search))
}
