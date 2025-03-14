use dirs;
use dotenv::dotenv;
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use std::env;
use std::fs;
use std::path::PathBuf;

#[derive(Serialize, Deserialize, Default)]
pub struct GlobalConfig {
    pub api_key: Option<String>,
    pub server: Option<String>,
    pub auth_server: Option<String>,
}

impl GlobalConfig {
    pub fn write(&self) -> Result<(), Box<dyn std::error::Error>> {
        let config_path = get_config_file_path()?;

        // Create parent directories if they don't exist
        if let Some(parent) = config_path.parent() {
            fs::create_dir_all(parent)?;
        }

        // Serialize the configuration to YAML and write to the file
        let yaml = serde_yaml::to_string(self)?;
        fs::write(config_path, yaml)?;

        Ok(())
    }

    pub fn read() -> Result<Self, Box<dyn std::error::Error>> {
        let config_path = get_config_file_path()?;
        let path_exists = config_path.exists();
        let mut config = if path_exists {
            let yaml = fs::read_to_string(config_path)?;
            serde_yaml::from_str(&yaml)?
        } else {
            GlobalConfig::default()
        };

        // Try to get API key from environment if not in config
        if config.api_key.is_none() {
            config.api_key = env::var("NEBU_API_KEY")
                .or_else(|_| env::var("AGENTSEA_API_KEY"))
                .ok();
        }
        // Try to get server from environment if not in config
        if config.server.is_none() {
            config.server = env::var("NEBU_SERVER")
                .or_else(|_| env::var("AGENTSEA_SERVER"))
                .ok()
                .or(Some("https://nebu.agentlabs.xyz".to_string()));
        }
        // Try to get auth server from environment if not in config
        if config.auth_server.is_none() {
            config.auth_server = env::var("NEBU_AUTH_SERVER")
                .or_else(|_| env::var("AGENTSEA_AUTH_SERVER"))
                .ok()
                .or(Some("https://auth.hub.agentlabs.xyz".to_string()));
        }

        // Only write the config file if it doesn't exist yet
        if !path_exists {
            config.write()?;
        }

        Ok(config)
    }
}

fn get_config_file_path() -> Result<PathBuf, Box<dyn std::error::Error>> {
    let home_dir = dirs::home_dir().ok_or("Could not determine home directory")?;
    let config_dir = home_dir.join(".agentsea");
    let config_path = config_dir.join("nebu.yaml");
    Ok(config_path)
}

pub struct Config {
    pub message_queue_type: String,
    pub kafka_bootstrap_servers: String,
    pub kafka_timeout_ms: String,
    pub redis_url: String,
    pub database_url: String,
}

impl Config {
    fn new() -> Self {
        dotenv().ok();

        Self {
            message_queue_type: env::var("MESSAGE_QUEUE_TYPE")
                .unwrap_or_else(|_| "redis".to_string()),
            kafka_bootstrap_servers: env::var("KAFKA_BOOTSTRAP_SERVERS")
                .unwrap_or_else(|_| "localhost:9092".to_string()),
            kafka_timeout_ms: env::var("KAFKA_TIMEOUT_MS").unwrap_or_else(|_| "5000".to_string()),
            redis_url: env::var("REDIS_URL").unwrap_or_else(|_| "redis://127.0.0.1/".to_string()),
            database_url: env::var("DATABASE_URL")
                .unwrap_or_else(|_| "sqlite://.data/data.db".to_string()),
        }
    }
}
// Global static CONFIG instance
pub static CONFIG: Lazy<Config> = Lazy::new(Config::new);
