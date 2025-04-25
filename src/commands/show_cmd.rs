use anyhow::{Context, Result};
use colored::Colorize;
use nebulous::config::GlobalConfig;
use std::error::Error;

pub async fn show_config() -> Result<(), Box<dyn Error>> {
    let config = GlobalConfig::read()?;

    println!("{}", "Global Configuration:".bold().underline());

    if let Some(current_server) = &config.current_server {
        println!("Current server: {}", current_server.green());
    } else {
        println!("Current server: {}", "None".yellow());
    }

    println!("\n{}", "Configured Servers:".bold());

    if config.servers.is_empty() {
        println!("  {}", "No servers configured".yellow());
    } else {
        for (idx, server) in config.servers.iter().enumerate() {
            let is_current = config
                .current_server
                .as_ref()
                .and_then(|current| server.name.as_ref().map(|name| current == name))
                .unwrap_or(false);

            let prefix = if is_current {
                "→ ".green()
            } else {
                "  ".normal()
            };

            println!("{}{}", prefix, format!("Server #{}", idx + 1).bold());

            if let Some(name) = &server.name {
                println!("{}  Name: {}", prefix, name);
            }

            if let Some(server_url) = &server.server {
                println!("{}  Server URL: {}", prefix, server_url);
            }

            if let Some(auth_server) = &server.auth_server {
                println!("{}  Auth Server: {}", prefix, auth_server);
            }

            if let Some(api_key) = &server.api_key {
                let hidden_key = format!(
                    "{}...{}",
                    &api_key.chars().take(4).collect::<String>(),
                    &api_key
                        .chars()
                        .rev()
                        .take(4)
                        .collect::<String>()
                        .chars()
                        .rev()
                        .collect::<String>()
                );
                println!("{}  API Key: {}", prefix, hidden_key);
            }

            if idx < config.servers.len() - 1 {
                println!();
            }
        }
    }

    Ok(())
}
