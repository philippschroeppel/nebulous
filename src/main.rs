mod cli;
mod commands;
mod models;

use crate::cli::{Cli, Commands, CreateCommands, DeleteCommands, GetCommands};
use clap::Parser;
use std::error::Error;
use tracing_subscriber;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    // Initialize tracing
    tracing_subscriber::fmt::init();

    // Parse command-line arguments
    let cli = Cli::parse();

    match cli.command {
        Commands::Serve { host, port } => {
            commands::serve_cmd::execute(host, port).await?;
        }
        Commands::Sync {
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
        Commands::Create { command } => match command {
            CreateCommands::Container { command } => {
                // Explicitly specify which ContainerCommands type to use
                commands::create_cmd::create_container(command).await?;
            }
        },
        Commands::Get { command } => match command {
            GetCommands::Accelerators { platform } => {
                commands::get_cmd::get_accelerators(platform).await?;
            }
            GetCommands::Containers { name } => {
                commands::get_cmd::get_containers(name).await?;
            }
            GetCommands::Platforms => {
                commands::get_cmd::get_platforms().await?;
            }
        },
        Commands::Delete { command } => match command {
            DeleteCommands::Containers { id } => {
                commands::delete_cmd::delete_container(id).await?;
            }
        },
        Commands::Login => {
            commands::login_cmd::execute().await?;
        }
    }

    Ok(())
}
