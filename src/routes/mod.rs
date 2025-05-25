use crate::auth::server::handlers::{get_api_key, list_api_keys};
use crate::handlers::v1::{
    create_container, create_namespace, create_processor, create_scoped_s3_token, create_secret,
    create_volume, delete_cache_key, delete_container, delete_container_by_id, delete_namespace,
    delete_processor, delete_scoped_s3_token, delete_secret, delete_secret_by_id, delete_volume,
    fetch_container_logs, fetch_container_logs_by_id, generate_temp_s3_credentials, get_cache_key,
    get_container, get_container_by_id, get_namespace, get_processor, get_processor_logs,
    get_secret, get_secret_by_id, get_user_profile, get_volume, list_cache_keys, list_containers,
    list_namespaces, list_processors, list_secrets, list_volumes, patch_container,
    read_processor_stream, scale_processor, search_containers, send_processor, stream_logs_ws,
    stream_logs_ws_by_id, update_processor, update_secret, update_secret_by_id,
};
use crate::handlers::{health_handler, root_handler};
use crate::middleware::auth_middleware;
use crate::state::AppState;
use axum::{
    middleware,
    routing::{delete, get, post},
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
        .route("/auth/api-key/:id", get(get_api_key))
        .route("/auth/api-keys", get(list_api_keys))
        .route(
            "/v1/auth/s3-tokens/:namespace/:name",
            get(create_scoped_s3_token),
        )
        .route(
            "/v1/auth/s3-tokens/:namespace/:name",
            delete(delete_scoped_s3_token),
        )
        .route(
            "/v1/auth/temp-s3-tokens/:namespace/:name",
            get(generate_temp_s3_credentials),
        )
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
        .route("/v1/containers/:id/logs/stream", get(stream_logs_ws_by_id))
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
        .route(
            "/v1/containers/:namespace/:name/logs/stream",
            get(stream_logs_ws),
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
        .route("/v1/volumes", get(list_volumes).post(create_volume))
        .route(
            "/v1/volumes/:namespace/:name",
            get(get_volume).delete(delete_volume),
        )
        .route(
            "/v1/processors",
            get(list_processors).post(create_processor),
        )
        .route(
            "/v1/processors/:namespace/:name",
            get(get_processor)
                .delete(delete_processor)
                .patch(update_processor),
        )
        .route(
            "/v1/processors/:namespace/:name/messages",
            post(send_processor),
        )
        .route(
            "/v1/processors/:namespace/:name/scale",
            post(scale_processor),
        )
        .route(
            "/v1/processors/:namespace/:name/logs",
            get(get_processor_logs),
        )
        .route(
            "/v1/processors/:namespace/:name/stream",
            post(read_processor_stream),
        )
        .route("/v1/cache", get(list_cache_keys))
        .route(
            "/v1/cache/:namespace/:key",
            get(get_cache_key).delete(delete_cache_key),
        )
        .route("/v1/users/me", get(get_user_profile))
        .route(
            "/v1/namespaces",
            get(list_namespaces).post(create_namespace),
        )
        .route(
            "/v1/namespaces/:name",
            get(get_namespace).delete(delete_namespace),
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
