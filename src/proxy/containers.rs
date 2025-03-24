use crate::query::Query;
use axum::body::Body;

use axum::{
    body::Bytes,
    extract::{Path, State},
    http::{HeaderMap, Method, StatusCode},
    middleware,
    middleware::from_fn,
    response::{IntoResponse, Response},
    routing::any,
    Router,
};
use serde_json::Value;
use tower_http::trace::TraceLayer;

use crate::middleware::auth_middleware; // <-- from your snippet in middleware.rs
use crate::AppState; // <-- your application state type

#[allow(dead_code)]
async fn forward_proxy(
    State(_app_state): State<AppState>, // replace with actual state usage if needed
    Path((namespace, name)): Path<(String, String)>,
    method: Method,
    headers: HeaderMap,
    body: Bytes,
) -> impl IntoResponse {
    //
    // 1) Lookup the container in the DB
    //
    let container_model =
        match Query::find_container_by_namespace_and_name(&_app_state.db_pool, &namespace, &name)
            .await
        {
            // Found a container row
            Ok(Some(c)) => c,
            // No container found
            Ok(None) => {
                return (StatusCode::NOT_FOUND, "No container found").into_response();
            }
            // DB error
            Err(e) => {
                eprintln!("[PROXY] Database error: {e}");
                return (StatusCode::INTERNAL_SERVER_ERROR, "Database error").into_response();
            }
        };

    //
    // 2) Parse and print the meters (or parse the entire container).
    //
    let meters = match container_model.parse_meters() {
        Ok(meters) => {
            println!(
                "[PROXY] Meters for container '{}.{}': {:?}",
                namespace, name, meters
            );
            meters
        }
        Err(e) => {
            eprintln!("[PROXY] Failed to parse meters: {e}");
            return (StatusCode::BAD_REQUEST, "Invalid meter data in container").into_response();
        }
    };

    // If this is a JSON request, parse and print the JSON body
    if let Some(content_type) = headers.get("content-type") {
        if content_type
            .to_str()
            .unwrap_or_default()
            .to_ascii_lowercase()
            .contains("application/json")
        {
            if let Ok(json_body) = serde_json::from_slice::<Value>(&body) {
                println!("[PROXY] ➡️ Request JSON Body: {json_body}");
            }
        }
    }

    let target_url = format!("http://{}.{}.container.nebu", namespace, name);
    let client = reqwest::Client::new();

    // Build outbound request with the same method, body, and forwarded headers
    let mut req_builder = client.request(method.clone(), &target_url).body(body);

    // Forward headers, skipping "transfer-encoding" if needed
    for (key, value) in headers.iter() {
        if key.as_str().eq_ignore_ascii_case("transfer-encoding") {
            continue;
        }
        req_builder = req_builder.header(key, value);
    }

    // Send the request
    match req_builder.send().await {
        Ok(resp) => {
            let status = resp.status();
            let resp_headers = resp.headers().clone();
            let bytes = resp.bytes().await.unwrap_or_else(|_| Bytes::new());

            // If response is JSON, parse and print
            if let Some(content_type) = resp_headers.get("content-type") {
                if content_type
                    .to_str()
                    .unwrap_or_default()
                    .to_ascii_lowercase()
                    .contains("application/json")
                {
                    if let Ok(json_resp) = serde_json::from_slice::<Value>(&bytes) {
                        println!("[PROXY] ⬅️ Response JSON Body: {json_resp}");
                    }
                }
            }

            // Build an Axum response
            let mut response = Response::builder().status(status);
            for (key, value) in resp_headers.iter() {
                response = response.header(key, value);
            }
            response.body(Body::from(bytes)).unwrap().into_response()
        }
        Err(e) => {
            eprintln!("[PROXY] ❌ Forwarding error: {e}");
            (StatusCode::BAD_GATEWAY, "Failed to forward request").into_response()
        }
    }
}

pub async fn start_proxy(app_state: AppState, port: u16) -> std::io::Result<()> {
    // Any route covers GET, POST, PUT, DELETE, etc.
    let app = Router::new()
        .route("/v1/containers/:namespace/:name/proxy", any(forward_proxy))
        .layer(middleware::from_fn_with_state(
            app_state.clone(),
            auth_middleware,
        ))
        // (Optional) Add any additional layers, like request tracing
        .layer(TraceLayer::new_for_http())
        // Provide the shared app state
        .with_state(app_state);

    // Run it
    let addr = format!("{}:{}", "0.0.0.0", port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    println!("Proxy server running at http://{}", addr);
    axum::serve(listener, app).await?;

    Ok(())
}
