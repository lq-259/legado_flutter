use axum::{
    extract::{Json, State},
    routing::get,
    Router,
};
use serde::{Deserialize, Serialize};

use crate::error::ApiError;
use crate::state::AppState;
use crate::util;

#[derive(Debug, Deserialize)]
pub struct ExploreRequest {
    pub source_id: String,
    pub explore_url: String,
    #[serde(default = "default_page")]
    pub page: i32,
}

fn default_page() -> i32 {
    1
}

#[derive(Debug, Serialize)]
pub struct ExploreResponse {
    pub items: Vec<core_source::parser::SearchResult>,
    pub page: i32,
}

async fn explore(
    State(state): State<AppState>,
    Json(req): Json<ExploreRequest>,
) -> Result<Json<ExploreResponse>, ApiError> {
    let mut conn = crate::util::open_db(&state.db_path)?;
    let source_dao = core_storage::source_dao::SourceDao::new(&mut conn);
    let source = source_dao
        .get_by_id(&req.source_id)
        .map_err(|e| ApiError::Database(e.to_string()))?
        .ok_or_else(|| ApiError::NotFound(format!("书源不存在: {}", req.source_id)))?;

    let core_source = util::storage_to_core_source(&source)?;
    let parser = core_source::parser::BookSourceParser::new();

    let results = parser
        .explore(&core_source, &req.explore_url, req.page)
        .await;

    Ok(Json(ExploreResponse {
        items: results,
        page: req.page,
    }))
}

#[derive(Debug, Deserialize)]
pub struct ListExploreRequest {
    pub source_id: String,
}

async fn list_explore_entries(
    State(state): State<AppState>,
    Json(req): Json<ListExploreRequest>,
) -> Result<Json<Vec<core_source::parser::ExploreEntry>>, ApiError> {
    let mut conn = crate::util::open_db(&state.db_path)?;
    let source_dao = core_storage::source_dao::SourceDao::new(&mut conn);
    let source = source_dao
        .get_by_id(&req.source_id)
        .map_err(|e| ApiError::Database(e.to_string()))?
        .ok_or_else(|| ApiError::NotFound(format!("书源不存在: {}", req.source_id)))?;

    let core_source = util::storage_to_core_source(&source)?;
    let entries = core_source::parser::BookSourceParser::get_explore_entries(&core_source);

    Ok(Json(entries))
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/explore", get(list_explore_entries).post(explore))
}
