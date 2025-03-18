// src/handlers/mod.rs

pub mod basic;
pub use basic::{health_handler, root_handler};
pub mod container;
pub use container::{
    create_container, delete_container, fetch_container_logs, get_container, list_containers,
};
pub mod secrets;
pub use secrets::{create_secret, delete_secret, get_secret, list_secrets, update_secret};
