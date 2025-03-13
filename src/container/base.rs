use crate::models::{V1Container, V1ContainerRequest};
use sea_orm::DatabaseConnection;
use std::collections::HashMap;
use std::env;
use std::fmt;
use std::str::FromStr;

/// Enum for container status
#[derive(Debug, serde::Serialize, serde::Deserialize, Clone, PartialEq)]
pub enum ContainerStatus {
    Defined,
    Restarting,
    Exited,
    Paused,
    Pending,
    Running,
    Completed,
    Failed,
    Stopped,
}

impl fmt::Display for ContainerStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ContainerStatus::Defined => write!(f, "defined"),
            ContainerStatus::Restarting => write!(f, "restarting"),
            ContainerStatus::Exited => write!(f, "exited"),
            ContainerStatus::Paused => write!(f, "paused"),
            ContainerStatus::Pending => write!(f, "pending"),
            ContainerStatus::Running => write!(f, "running"),
            ContainerStatus::Completed => write!(f, "completed"),
            ContainerStatus::Failed => write!(f, "failed"),
            ContainerStatus::Stopped => write!(f, "stopped"),
        }
    }
}

impl FromStr for ContainerStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "defined" => Ok(ContainerStatus::Defined),
            "restarting" => Ok(ContainerStatus::Restarting),
            "exited" => Ok(ContainerStatus::Exited),
            "paused" => Ok(ContainerStatus::Paused),
            "pending" => Ok(ContainerStatus::Pending),
            "running" => Ok(ContainerStatus::Running),
            "completed" => Ok(ContainerStatus::Completed),
            "failed" => Ok(ContainerStatus::Failed),
            "stopped" => Ok(ContainerStatus::Stopped),
            _ => Err(format!("Unknown container status: {}", s)),
        }
    }
}

pub trait ContainerPlatform {
    async fn run(
        &self,
        config: &V1ContainerRequest,
        db: &DatabaseConnection,
        owner_id: &str,
    ) -> Result<V1Container, Box<dyn std::error::Error>>;

    async fn delete(
        &self,
        id: &str,
        db: &DatabaseConnection,
    ) -> Result<(), Box<dyn std::error::Error>>;

    fn accelerator_map(&self) -> HashMap<String, String>;

    // Default implementation for common environment variables
    fn get_common_env_vars(&self) -> HashMap<String, String> {
        let mut env_vars = HashMap::new();

        // Get AWS credentials from environment
        let aws_access_key =
            env::var("AWS_ACCESS_KEY_ID").expect("AWS_ACCESS_KEY_ID environment variable not set");
        let aws_secret_key = env::var("AWS_SECRET_ACCESS_KEY")
            .expect("AWS_SECRET_ACCESS_KEY environment variable not set");

        // Add RCLONE environment variables
        env_vars.insert("RCLONE_CONFIG_S3REMOTE_TYPE".to_string(), "s3".to_string());
        env_vars.insert(
            "RCLONE_CONFIG_S3REMOTE_PROVIDER".to_string(),
            "AWS".to_string(),
        );
        env_vars.insert(
            "RCLONE_CONFIG_S3REMOTE_ENV_AUTH".to_string(),
            "true".to_string(),
        );
        env_vars.insert("AWS_ACCESS_KEY_ID".to_string(), aws_access_key);
        env_vars.insert("AWS_SECRET_ACCESS_KEY".to_string(), aws_secret_key);
        env_vars.insert(
            "RCLONE_CONFIG_S3REMOTE_REGION".to_string(),
            "us-east-1".to_string(),
        );
        // env_vars.insert(
        //     "RCLONE_CONFIG_S3REMOTE_ACL".to_string(),
        //     "private".to_string(),
        // );

        // Add more common environment variables as needed
        env_vars
    }
}
