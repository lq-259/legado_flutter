use axum::http::StatusCode;
use axum::response::{IntoResponse, Json, Response};
use serde_json::json;

#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("数据库错误: {0}")]
    Database(String),

    #[error("请求参数错误: {0}")]
    BadRequest(String),

    #[error("资源不存在: {0}")]
    NotFound(String),

    #[error("解析失败: {0}")]
    Parse(String),

    #[error("内部错误: {0}")]
    Internal(String),
}

impl From<rusqlite::Error> for ApiError {
    fn from(e: rusqlite::Error) -> Self {
        ApiError::Database(e.to_string())
    }
}

impl From<serde_json::Error> for ApiError {
    fn from(e: serde_json::Error) -> Self {
        ApiError::Parse(e.to_string())
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status = match self {
            ApiError::BadRequest(_) => StatusCode::BAD_REQUEST,
            ApiError::NotFound(_) => StatusCode::NOT_FOUND,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        };
        let body = json!({ "error": self.to_string() });
        (status, Json(body)).into_response()
    }
}
