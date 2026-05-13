use axum::{extract::DefaultBodyLimit, Router};
use tokio::net::TcpListener;
use tracing_subscriber::EnvFilter;

mod error;
mod routes;
mod state;
mod util;

use state::AppState;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();

    let db_path =
        std::env::var("LEGADO_DB_PATH").unwrap_or_else(|_| "./legado.db".into());
    let host = std::env::var("LEGADO_HOST").unwrap_or_else(|_| "127.0.0.1".into());
    let port = std::env::var("LEGADO_PORT").unwrap_or_else(|_| "8787".into());
    let bind_addr = format!("{}:{}", host, port);

    tracing::info!("Initializing database at {}", db_path);
    core_storage::database::init_database(&db_path)
        .expect("Failed to initialize database");

    core_source::legado::js_runtime::set_cache_db_path(Some(db_path.clone()));

    let state = AppState { db_path };

    let app = Router::new()
        .nest("/", routes::routes())
        .layer(DefaultBodyLimit::max(5 * 1024 * 1024))
        .with_state(state);

    let listener = TcpListener::bind(&bind_addr)
        .await
        .expect("Failed to bind address");

    tracing::info!("API server listening on {}", bind_addr);
    axum::serve(listener, app).await.unwrap();
}
