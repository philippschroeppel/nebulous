// src/lib.rs

pub mod accelerator;
pub mod auth;
pub mod cli;
pub mod config;
pub mod db;
pub mod dns;
pub mod entities;
pub mod errors;
pub mod handlers;
pub mod middleware;
pub mod models;
pub mod mutation;
pub mod neblet;
pub mod oci;
pub mod org;
pub mod proxy;
pub mod query;
pub mod resources;
pub mod routes;
pub mod ssh;
pub mod state;
pub mod streams;
pub mod validate;
pub mod volumes;

use crate::config::CONFIG;
use axum::Router;
use db::init_db;
use rdkafka::admin::AdminClient;
use rdkafka::producer::FutureProducer;
use rdkafka::ClientConfig;
use routes::create_routes;
use state::AppState;
use state::MessageQueue;
use std::env;
use std::sync::Arc;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;
use url::Url;

/// Create and return the application state.
pub async fn create_app_state() -> Result<AppState, Box<dyn std::error::Error>> {
    println!("Creating app state");
    let db_pool = init_db().await?;
    println!("Database pool created");

    // Initialize the appropriate message queue based on configuration
    let message_queue = match CONFIG.message_queue_type.to_lowercase().as_str() {
        "redis" => {
            let redis_url = match &CONFIG.redis_url {
                Some(url) if !url.is_empty() => {
                    // Redis URL exists, so use it directly but also parse it to set env vars
                    if let Ok(parsed_url) = Url::parse(url) {
                        // Extract and set host
                        if let Some(host) = parsed_url.host_str() {
                            env::set_var("REDIS_HOST", host);
                        }

                        // Extract and set port
                        if let Some(port) = parsed_url.port() {
                            env::set_var("REDIS_PORT", port.to_string());
                        } else {
                            // Default redis port if not specified in URL
                            env::set_var("REDIS_PORT", "6379");
                        }

                        // Extract and set password if present
                        if let Some(password) = parsed_url.password() {
                            env::set_var("REDIS_PASSWORD", password);
                        }
                    }

                    url.clone()
                }
                _ => {
                    // Redis URL not present or empty, build from components
                    let host = &CONFIG.redis_host;
                    let port = &CONFIG.redis_port;

                    match &CONFIG.redis_password {
                        Some(password) if !password.is_empty() => {
                            format!("redis://:{}@{}:{}", password, host, port)
                        }
                        _ => format!("redis://{}:{}", host, port),
                    }
                }
            };

            // Create the Redis client using the constructed URL
            let redis_client = Arc::new(redis::Client::open(redis_url.as_str())?);

            MessageQueue::Redis {
                client: redis_client,
            }
        }
        "kafka" => {
            let mut client_config = ClientConfig::new();
            let kafka_config = client_config
                .set("bootstrap.servers", &CONFIG.kafka_bootstrap_servers)
                .set("message.timeout.ms", &CONFIG.kafka_timeout_ms);

            let producer = Arc::new(kafka_config.clone().create::<FutureProducer>()?);
            let admin = Arc::new(kafka_config.create::<AdminClient<_>>()?);

            MessageQueue::Kafka { producer, admin }
        }
        unsupported => {
            return Err(format!("Unsupported message queue type: {}", unsupported).into())
        }
    };

    let app_state = AppState {
        db_pool,
        message_queue,
    };

    Ok(app_state)
}

/// Given the `AppState`, create and return the Axum `Router`.
pub async fn create_app(app_state: AppState) -> Router {
    let routes = create_routes(app_state.clone());

    // Define a CORS layer (this example allows any origin, headers, and methods)
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let app = routes
        .layer(TraceLayer::new_for_http())
        .layer(cors)
        .with_state(app_state);

    app
}
