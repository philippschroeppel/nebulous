use crate::auth::server::handlers::{generate_api_key, get_api_key, list_api_keys, revoke_api_key};
use crate::state::AppState;
use axum::routing::{get, post};
use axum::Router;
use tower_http::trace::TraceLayer;
pub async fn health_check() -> &'static str {
    "OK"
}

pub async fn start_auth_server(app_state: AppState, port: u16) -> std::io::Result<()> {
    let app = Router::new()
        .route("/health", get(health_check))
        .route("/api-keys", get(list_api_keys))
        .route("/api-key/:id", get(get_api_key))
        .route("/api-key/generate", get(generate_api_key))
        .route("/api-key/revoke", post(revoke_api_key))
        .layer(TraceLayer::new_for_http())
        .with_state(app_state);

    let addr = format!("{}:{}", "127.0.0.1", port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    println!("Auth server running at http://{}", addr);

    axum::serve(listener, app).await?;
    Ok(())
}
