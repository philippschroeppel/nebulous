// src/lib.rs

pub mod accelerator;
pub mod cli;
pub mod config;
pub mod container;
pub mod db;
pub mod entities;
pub mod errors;
pub mod handlers;
pub mod middleware;
pub mod models;
pub mod mutation;
pub mod org;
pub mod query;
pub mod routes;
pub mod state;
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
use std::sync::Arc;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;

pub async fn create_app() -> Result<Router, Box<dyn std::error::Error>> {
    println!("Creating app");
    let db_pool = init_db().await?;
    println!("Database pool created");

    // Initialize the appropriate message queue based on configuration
    let message_queue = match CONFIG.message_queue_type.to_lowercase().as_str() {
        "redis" => {
            // Read the password from the REDIS_PASSWORD environment variable
            let redis_password = std::env::var("REDIS_PASSWORD")
                .expect("REDIS_PASSWORD environment variable not set");

            // Strip off the "redis://" prefix if it’s present to avoid duplicating the scheme
            let stripped_url = CONFIG
                .redis_url
                .strip_prefix("redis://")
                .unwrap_or_else(|| CONFIG.redis_url.as_str());

            // Construct a new connection URL that injects the password
            // e.g. if CONFIG.redis_url was "redis://localhost:6379",
            // final URL becomes "redis://:password@localhost:6379"
            let redis_url_with_password = format!("redis://:{redis_password}@{stripped_url}");

            // Create the Redis client using the new URL
            let redis_client = Arc::new(redis::Client::open(redis_url_with_password)?);

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
        db_pool: db_pool.clone(),
        message_queue,
    };

    // Create your routes
    let routes = create_routes(app_state.clone());

    // Define a CORS layer (this example allows any origin, headers, and methods)
    let cors = CorsLayer::new()
        .allow_origin(Any) // TODO: fix me
        .allow_methods(Any)
        .allow_headers(Any);

    let app = routes
        .layer(TraceLayer::new_for_http())
        .layer(cors)
        .with_state(app_state);

    Ok(app)
}
