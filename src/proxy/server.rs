use crate::models::V1UserProfile;
use crate::proxy::containers::forward_container;
use axum::debug_handler;
use axum::extract::OriginalUri;
use axum::routing::get;
use axum::{
    body::Bytes,
    extract::Extension,
    extract::State,
    http::{HeaderMap, Method, StatusCode},
    middleware,
    response::IntoResponse,
    routing::any,
    Router,
};
use tower_http::trace::TraceLayer;

use crate::middleware::auth_middleware;
use crate::AppState;
use tracing::debug;

#[allow(dead_code)]
#[debug_handler]
async fn forward_proxy(
    State(app_state): State<AppState>, // replace with actual state usage if needed
    Extension(user_profile): Extension<V1UserProfile>,
    method: Method,
    headers: HeaderMap,
    OriginalUri(original_uri): OriginalUri,
    body: Bytes,
) -> impl IntoResponse {
    debug!(
        "[PROXY] Forwarding proxy request to {:?}",
        headers.get("x-resource")
    );

    debug!("[PROXY] User profile: {:?}", user_profile);

    let mut owner_ids: Vec<String> = if let Some(orgs) = &user_profile.organizations {
        orgs.keys().cloned().collect()
    } else {
        Vec::new()
    };

    // Include user's email (assuming owner_id is user's email)
    owner_ids.push(user_profile.email.clone());
    let _owner_id_refs: Vec<&str> = owner_ids.iter().map(|s| s.as_str()).collect();

    // 1) Extract the namespace and name from custom headers
    let resource_str = match headers.get("x-resource") {
        Some(val) => match val.to_str() {
            Ok(ns_str) => ns_str.to_string(),
            Err(_) => {
                return (StatusCode::BAD_REQUEST, "Invalid x-resource header").into_response()
            }
        },
        None => return (StatusCode::BAD_REQUEST, "Missing x-resource header").into_response(),
    };

    // 2) Parse out <name>.<namespace>.<kind>
    let parts: Vec<&str> = resource_str.split('.').collect();
    if parts.len() != 3 {
        return (
            StatusCode::BAD_REQUEST,
            "Invalid x-resource format. Must be <name>.<namespace>.<kind>",
        )
            .into_response();
    }
    let (name, namespace, kind) = (parts[0], parts[1], parts[2]);

    debug!("[PROXY] Name: {:?}", name);
    debug!("[PROXY] Namespace: {:?}", namespace);
    debug!("[PROXY] Kind: {:?}", kind);

    match kind.to_lowercase().as_str() {
        "container" => forward_container(
            State(app_state),
            user_profile,
            namespace.to_string(),
            name.to_string(),
            method,
            headers,
            body,
            original_uri,
        )
        .await
        .into_response(),
        _ => (StatusCode::BAD_REQUEST, "Invalid kind in x-resource header").into_response(),
    }
}

pub async fn health_check() -> &'static str {
    "OK"
}

pub async fn start_proxy(app_state: AppState, port: u16) -> std::io::Result<()> {
    let app = Router::new()
        // Health route
        .route("/health", get(health_check))
        // Fallback to forward_proxy
        .fallback(any(forward_proxy))
        // Middlewares
        .layer(middleware::from_fn_with_state(
            app_state.clone(),
            auth_middleware,
        ))
        .layer(TraceLayer::new_for_http())
        // Provide shared state
        .with_state(app_state);

    // Run it
    let addr = format!("{}:{}", "0.0.0.0", port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    println!("Proxy server running at http://{}", addr);
    axum::serve(listener, app).await?;

    Ok(())
}
