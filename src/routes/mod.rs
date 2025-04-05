use crate::handlers::auth::{generate_api_key, get_api_key, list_api_keys, revoke_api_key};
use crate::handlers::v1::{
    create_container, create_secret, delete_container, delete_container_by_id, delete_secret,
    delete_secret_by_id, fetch_container_logs, fetch_container_logs_by_id, get_container,
    get_container_by_id, get_secret, get_secret_by_id, list_containers, list_secrets,
    patch_container, search_containers, update_secret, update_secret_by_id,
};
use crate::handlers::{health_handler, root_handler};
use crate::middleware::auth_middleware;
use crate::state::AppState;
use axum::{
    middleware,
    routing::{get, post},
    Router,
};
use tower_http::trace::{self, TraceLayer};
use tracing::Level;

pub fn create_routes(app_state: AppState) -> Router<AppState> {
    // Public routes that do not require authentication
    let public_routes = Router::new()
        .route("/", get(root_handler))
        .route("/health", get(health_handler));

    // Private routes that require authentication
    let private_routes = Router::new()
        .route("/auth/api-keys/:id", get(get_api_key))
        .route("/auth/api-keys", get(list_api_keys))
        .route("/auth/api-keys/generate", get(generate_api_key))
        .route("/auth/api-keys/revoke", post(revoke_api_key))
        .route(
            "/v1/containers",
            get(list_containers).post(create_container),
        )
        .route("/v1/containers/search", post(search_containers))
        .route(
            "/v1/containers/:id",
            get(get_container_by_id).delete(delete_container_by_id),
        )
        .route("/v1/containers/:id/logs", get(fetch_container_logs_by_id))
        .route(
            "/v1/containers/:namespace/:name",
            get(get_container)
                .delete(delete_container)
                .patch(patch_container),
        )
        .route(
            "/v1/containers/:namespace/:name/logs",
            get(fetch_container_logs),
        )
        .route("/v1/secrets", get(list_secrets).post(create_secret))
        .route(
            "/v1/secrets/:id",
            get(get_secret_by_id)
                .put(update_secret_by_id)
                .delete(delete_secret_by_id),
        )
        .route(
            "/v1/secrets/:namespace/:name",
            get(get_secret).delete(delete_secret).put(update_secret),
        )
        // Apply the authentication middleware to private routes
        .layer(middleware::from_fn_with_state(
            app_state.clone(),
            auth_middleware,
        ));

    // Combine public and private routes
    public_routes.merge(private_routes).layer(
        TraceLayer::new_for_http()
            .make_span_with(trace::DefaultMakeSpan::new().level(Level::INFO))
            .on_response(trace::DefaultOnResponse::new().level(Level::INFO)),
    )
}
