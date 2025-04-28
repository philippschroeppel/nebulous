use std::env;
use std::fs::File;
use std::io::Read;
use std::path::PathBuf;

use dirs;
use serde::{Deserialize, Serialize};
use serde_yaml;

/// Rust equivalent of the Python OrignServerConfig dataclass
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ServerConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auth_server: Option<String>,
}

/// Configuration container for multiple servers
#[derive(Debug, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub servers: Vec<ServerConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_server: Option<String>,
}

/// Get the Orign server URL from environment variables or config file
pub fn get_orign_server() -> Option<String> {
    // Check environment variables first
    if let Ok(server) = env::var("ORIGN_SERVER") {
        return Some(server);
    }
    if let Ok(server) = env::var("ORIGIN_SERVER") {
        return Some(server);
    }

    // Check config file
    let path = get_config_file_path();
    if path.exists() {
        return load_server_from_config(&path);
    }

    None
}

/// Load the server URL from the config file
fn load_server_from_config(path: &PathBuf) -> Option<String> {
    match File::open(path) {
        Ok(mut file) => {
            let mut contents = String::new();
            if file.read_to_string(&mut contents).is_err() {
                eprintln!("Warning: Could not read config file '{}'", path.display());
                return None;
            }

            match serde_yaml::from_str::<Config>(&contents) {
                Ok(config) => {
                    if let Some(current_server) = &config.current_server {
                        for server in config.servers {
                            if let Some(name) = &server.name {
                                if name == current_server {
                                    return server.server;
                                }
                            }
                        }
                    }
                    None
                }
                Err(e) => {
                    eprintln!(
                        "Warning: Could not parse config file '{}': {}. Starting with default config.",
                        path.display(),
                        e
                    );
                    None
                }
            }
        }
        Err(e) => {
            eprintln!(
                "Warning: Could not open config file '{}': {}. Starting with default config.",
                path.display(),
                e
            );
            None
        }
    }
}

/// Return the path to ~/.agentsea/orign.yaml
fn get_config_file_path() -> PathBuf {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    let config_dir = home.join(".agentsea");
    config_dir.join("orign.yaml")
}
