use axum::{
    extract::{Json, Path, State},
    routing::{delete, get},
    Router,
};
use serde::Deserialize;

use crate::error::ApiError;
use crate::state::AppState;
use crate::util;

#[derive(Debug, Deserialize)]
pub struct CreateReplaceRuleRequest {
    pub name: String,
    pub pattern: String,
    pub replacement: String,
    pub scope: Option<i32>,
}

async fn list_rules(State(state): State<AppState>) -> Result<Json<serde_json::Value>, ApiError> {
    let conn = util::open_db(&state.db_path)?;
    let dao = core_storage::replace_rule_dao::ReplaceRuleDao::new(&conn);
    let rules = dao
        .get_all()
        .map_err(|e| ApiError::Database(e.to_string()))?;
    Ok(Json(serde_json::to_value(rules)?))
}

async fn list_enabled_rules(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let conn = util::open_db(&state.db_path)?;
    let dao = core_storage::replace_rule_dao::ReplaceRuleDao::new(&conn);
    let rules = dao
        .get_enabled()
        .map_err(|e| ApiError::Database(e.to_string()))?;
    Ok(Json(serde_json::to_value(rules)?))
}

async fn create_rule(
    State(state): State<AppState>,
    Json(req): Json<CreateReplaceRuleRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    if req.name.trim().is_empty() {
        return Err(ApiError::BadRequest("规则名称不能为空".into()));
    }
    let conn = util::open_db(&state.db_path)?;
    let dao = core_storage::replace_rule_dao::ReplaceRuleDao::new(&conn);
    let rule = dao
        .create(
            &req.name,
            &req.pattern,
            &req.replacement,
            req.scope.unwrap_or(0),
        )
        .map_err(|e| ApiError::Database(e.to_string()))?;
    Ok(Json(serde_json::to_value(rule)?))
}

async fn delete_rule(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let conn = util::open_db(&state.db_path)?;
    let dao = core_storage::replace_rule_dao::ReplaceRuleDao::new(&conn);
    dao.get_by_id(&id)
        .map_err(|e| ApiError::Database(e.to_string()))?
        .ok_or_else(|| ApiError::NotFound(format!("替换规则不存在: {}", id)))?;
    dao.delete(&id)
        .map_err(|e| ApiError::Database(e.to_string()))?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/replace-rules", get(list_rules).post(create_rule))
        .route("/api/replace-rules/enabled", get(list_enabled_rules))
        .route("/api/replace-rules/:id", delete(delete_rule))
}
