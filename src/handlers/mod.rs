// src/handlers/mod.rs

pub mod basic;
pub use basic::{health_handler, root_handler};
pub mod auth;
pub mod v1;
