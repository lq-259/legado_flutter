mod bookshelf;
mod explore;
mod health;
mod reader;
mod replace_rules;
mod search;
mod sources;

use axum::{routing::get, Router};

use crate::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/health", get(health::health))
        .merge(sources::routes())
        .merge(search::routes())
        .merge(bookshelf::routes())
        .merge(reader::routes())
        .merge(replace_rules::routes())
        .merge(explore::routes())
}
