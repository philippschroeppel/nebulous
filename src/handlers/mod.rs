// src/handlers/mod.rs

pub mod basic;
pub use basic::{health_handler, root_handler};
pub mod container;
pub use container::{
    create_container, delete_container, delete_container_by_id, fetch_container_logs,
    get_container, get_container_by_id, list_containers,
};
pub mod secrets;
pub use secrets::{create_secret, delete_secret, get_secret, list_secrets, update_secret};
