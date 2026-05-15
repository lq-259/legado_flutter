use axum::{
    extract::DefaultBodyLimit,
    extract::{Request, State},
    http::StatusCode,
    middleware::{self, Next},
    response::Response,
    Router,
};
use tokio::net::TcpListener;
use tracing_subscriber::EnvFilter;

mod error;
mod routes;
mod state;
mod util;

use state::AppState;

fn is_loopback(host: &str) -> bool {
    host == "127.0.0.1"
        || host == "localhost"
        || host == "::1"
        || host.starts_with("127.")
        || host == "[::1]"
        || host
            .parse::<std::net::IpAddr>()
            .is_ok_and(|ip| ip.is_loopback())
}

async fn auth_middleware(
    State(state): State<AppState>,
    req: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    if let Some(ref token) = state.api_token {
        let auth = req
            .headers()
            .get("Authorization")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        if auth != format!("Bearer {}", token) {
            return Err(StatusCode::UNAUTHORIZED);
        }
    }
    Ok(next.run(req).await)
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();

    let db_path = std::env::var("LEGADO_DB_PATH").unwrap_or_else(|_| "./legado.db".into());
    let host = std::env::var("LEGADO_HOST").unwrap_or_else(|_| "127.0.0.1".into());
    let port = std::env::var("LEGADO_PORT").unwrap_or_else(|_| "8787".into());
    let api_token = std::env::var("LEGADO_API_TOKEN")
        .ok()
        .filter(|s| !s.is_empty());
    let bind_addr = format!("{}:{}", host, port);

    if !is_loopback(&host) && api_token.is_none() {
        panic!(
            "LEGADO_HOST={} is not loopback; LEGADO_API_TOKEN must be set",
            host
        );
    }

    tracing::info!("Initializing database at {}", db_path);
    core_storage::database::init_database(&db_path).expect("Failed to initialize database");

    core_source::legado::js_runtime::set_cache_db_path(Some(db_path.clone()));

    let state = AppState { db_path, api_token };

    let mut app = Router::new()
        .nest("/", routes::routes())
        .layer(DefaultBodyLimit::max(5 * 1024 * 1024));

    if state.api_token.is_some() {
        app = app.layer(middleware::from_fn_with_state(
            state.clone(),
            auth_middleware,
        ));
    }

    let app = app.with_state(state);

    let listener = TcpListener::bind(&bind_addr)
        .await
        .expect("Failed to bind address");

    tracing::info!("API server listening on {}", bind_addr);
    axum::serve(listener, app).await.unwrap();
}
