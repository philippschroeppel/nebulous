mod cli;
mod commands;
mod models;

use crate::cli::{Cli, Commands, CreateCommands, DeleteCommands, GetCommands, ProxyCommands};
use clap::Parser;
use cli::SyncCommands;
use std::error::Error;
use tracing_subscriber;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    // Initialize logging with INFO level
    if std::env::var("RUST_LOG").is_err() {
        std::env::set_var("RUST_LOG", "info");
    }
    // Initialize tracing
    tracing_subscriber::fmt::init();

    // Parse command-line arguments
    let cli = Cli::parse();

    match cli.command {
        Commands::Serve { host, port } => {
            commands::serve_cmd::execute(host, port).await?;
        }
        Commands::Sync { command } => match command {
            SyncCommands::Volumes {
                config,
                interval_seconds,
                create_if_missing,
                watch,
                background,
                block_once,
                config_from_env,
            } => {
                commands::sync_cmd::execute_sync(
                    config,
                    interval_seconds,
                    create_if_missing,
                    watch,
                    background,
                    block_once,
                    config_from_env,
                )
                .await?;
            }
            SyncCommands::Wait {
                config,
                interval_seconds,
            } => {
                commands::sync_cmd::execute_wait(&config, interval_seconds).await?;
            }
        },
        Commands::Create { command } => match command {
            CreateCommands::Containers { command } => {
                // Add debug output to help diagnose the issue
                println!("Attempting to create container with command");

                // Wrap the call in a match to catch and print any errors
                match commands::create_cmd::create_container(command).await {
                    Ok(_) => (),
                    Err(e) => {
                        eprintln!("Error creating container: {:?}", e);
                        return Err(e);
                    }
                }
            }
        },
        Commands::Get { command } => match command {
            GetCommands::Accelerators { platform } => {
                commands::get_cmd::get_accelerators(platform).await?;
            }
            GetCommands::Containers { id } => {
                commands::get_cmd::get_containers(id).await?;
            }
            GetCommands::Platforms => {
                commands::get_cmd::get_platforms().await?;
            }
            GetCommands::Secrets { id } => {
                commands::get_cmd::get_secrets(id).await?;
            }
        },
        Commands::Delete { command } => match command {
            DeleteCommands::Containers { id } => {
                commands::delete_cmd::delete_container(id).await?;
            }
        },
        Commands::Proxy { command } => match command {
            ProxyCommands::Shell { host, port } => {
                commands::proxy_cmd::run_sync_cmd_server(&host, port).await?;
            }
        },
        Commands::Login => {
            commands::login_cmd::execute().await?;
        }
    }

    Ok(())
}
