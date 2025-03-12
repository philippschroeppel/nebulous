use crate::models::V1UserProfile;
use crate::AppState;
use axum::{
    extract::{Request, State},
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;

pub async fn auth_middleware(
    State(_app_state): State<AppState>,
    mut request: Request,
    next: Next,
) -> Response {
    // Extract the Authorization header
    let auth_header = match request.headers().get("Authorization") {
        Some(header) => header.to_str().unwrap_or(""),
        None => {
            return unauthorized_response();
        }
    };

    println!("🔐 Making auth request to: https://auth.hub.agentlabs.xyz/v1/users/me");

    // Validate the token with agentlabs
    let client = reqwest::Client::new();
    let user_profile_result = client
        .get("https://auth.hub.agentlabs.xyz/v1/users/me")
        .header("Authorization", auth_header)
        .send()
        .await;

    match user_profile_result {
        Ok(response) => {
            if response.status().is_success() {
                // Clone the response so we can read the body twice
                let response_text = response.text().await.unwrap_or_default();
                println!("✅ Auth response: {}", response_text);

                // Parse the user profile from the cloned response
                match serde_json::from_str::<V1UserProfile>(&response_text) {
                    Ok(user_profile) => {
                        request.extensions_mut().insert(user_profile);
                        return next.run(request).await;
                    }
                    Err(e) => {
                        println!("❌ Failed to parse user profile: {}", e);
                        unauthorized_response()
                    }
                }
            } else {
                println!("❌ Auth failed with status: {}", response.status());
                unauthorized_response()
            }
        }
        Err(e) => {
            println!("❌ Auth request failed: {}", e);
            unauthorized_response()
        }
    }
}

fn unauthorized_response() -> Response {
    let error_response = json!({
        "error": {
            "message": "Unauthorized",
            "type": "authentication_error",
            "param": null,
            "code": null
        }
    });
    (StatusCode::UNAUTHORIZED, Json(error_response)).into_response()
}
