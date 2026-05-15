#[derive(Clone)]
pub struct AppState {
    pub db_path: String,
    pub api_token: Option<String>,
}
