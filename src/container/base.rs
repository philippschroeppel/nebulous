use crate::auth::agent::create_agent_key;
use crate::entities::containers;
use crate::models::{V1Container, V1ContainerRequest, V1CreateAgentKeyRequest, V1UserProfile};
use crate::query::Query;
use sea_orm::DatabaseConnection;
use std::collections::HashMap;
use std::env;
use std::fmt;
use std::str::FromStr;
use tracing::info;

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
    Invalid,
    Creating,
    Created,
    Queued,
}

impl ContainerStatus {
    /// Returns a list of all statuses considered inactive (terminal).
    pub fn inactive() -> Vec<Self> {
        vec![
            ContainerStatus::Completed,
            ContainerStatus::Failed,
            ContainerStatus::Stopped,
            ContainerStatus::Exited,
            ContainerStatus::Invalid,
        ]
    }

    /// Returns a list of all statuses considered active (non-terminal).
    pub fn active() -> Vec<Self> {
        vec![
            ContainerStatus::Defined,
            ContainerStatus::Restarting,
            ContainerStatus::Paused,
            ContainerStatus::Pending,
            ContainerStatus::Running,
            ContainerStatus::Creating,
            ContainerStatus::Created,
            ContainerStatus::Queued,
        ]
    }

    pub fn needs_start(&self) -> bool {
        matches!(
            self,
            ContainerStatus::Defined
                | ContainerStatus::Paused
                | ContainerStatus::Pending
                | ContainerStatus::Queued
        )
    }

    pub fn needs_watch(&self) -> bool {
        matches!(
            self,
            ContainerStatus::Running
                | ContainerStatus::Creating
                | ContainerStatus::Created
                | ContainerStatus::Restarting
        )
    }

    /// Returns true if the container is in a terminal (inactive) state.
    pub fn is_inactive(&self) -> bool {
        matches!(
            self,
            ContainerStatus::Completed
                | ContainerStatus::Failed
                | ContainerStatus::Stopped
                | ContainerStatus::Exited
                | ContainerStatus::Invalid
        )
    }

    /// Returns true if the container is in an active (non-terminal) state.
    pub fn is_active(&self) -> bool {
        !self.is_inactive()
    }
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
            ContainerStatus::Invalid => write!(f, "invalid"),
            ContainerStatus::Creating => write!(f, "creating"),
            ContainerStatus::Created => write!(f, "created"),
            ContainerStatus::Queued => write!(f, "queued"),
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
            "creating" => Ok(ContainerStatus::Creating),
            "created" => Ok(ContainerStatus::Created),
            "queued" => Ok(ContainerStatus::Queued),
            _ => Err(format!("Unknown container status: {}", s)),
        }
    }
}

pub trait ContainerPlatform {
    async fn declare(
        &self,
        config: &V1ContainerRequest,
        db: &DatabaseConnection,
        user_profile: &V1UserProfile,
        owner_id: &str,
    ) -> Result<V1Container, Box<dyn std::error::Error + Send + Sync>>;

    async fn reconcile(
        &self,
        container: &containers::Model,
        db: &DatabaseConnection,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>>;

    async fn exec(
        &self,
        container_id: &str,
        command: &str,
        db: &DatabaseConnection,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>>;

    async fn logs(
        &self,
        container_id: &str,
        db: &DatabaseConnection,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>>;

    async fn delete(
        &self,
        id: &str,
        db: &DatabaseConnection,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>>;

    fn accelerator_map(&self) -> HashMap<String, String>;

    // Default implementation for common environment variables
    async fn get_common_env(
        &self,
        model: &containers::Model,
        db: &DatabaseConnection,
    ) -> HashMap<String, String> {
        let config = crate::config::GlobalConfig::read().unwrap();
        let mut env = HashMap::new();

        let agent_key = Query::get_agent_key(db, model.id.clone()).await.unwrap();

        // Get AWS credentials from environment
        let aws_access_key =
            env::var("AWS_ACCESS_KEY_ID").expect("AWS_ACCESS_KEY_ID environment variable not set");
        let aws_secret_key = env::var("AWS_SECRET_ACCESS_KEY")
            .expect("AWS_SECRET_ACCESS_KEY environment variable not set");

        // Add RCLONE environment variables
        env.insert("RCLONE_CONFIG_S3REMOTE_TYPE".to_string(), "s3".to_string());
        env.insert(
            "RCLONE_CONFIG_S3REMOTE_PROVIDER".to_string(),
            "AWS".to_string(),
        );
        env.insert(
            "RCLONE_CONFIG_S3REMOTE_ENV_AUTH".to_string(),
            "true".to_string(),
        );
        env.insert("AWS_ACCESS_KEY_ID".to_string(), aws_access_key);
        env.insert("AWS_SECRET_ACCESS_KEY".to_string(), aws_secret_key);
        env.insert(
            "RCLONE_CONFIG_S3REMOTE_REGION".to_string(),
            "us-east-1".to_string(),
        );
        env.insert("NEBU_API_KEY".to_string(), agent_key.unwrap());
        env.insert("NEBU_SERVER".to_string(), config.server.unwrap());
        env.insert("HF_HOME".to_string(), "/nebu/cache/huggingface".to_string());

        // env.insert(
        //     "RCLONE_CONFIG_S3REMOTE_ACL".to_string(),
        //     "private".to_string(),
        // );

        // Add more common environment variables as needed
        env
    }

    async fn get_agent_key(
        &self,
        user_profile: &V1UserProfile,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        let config = crate::config::GlobalConfig::read().unwrap();

        let create_agent_key_request = V1CreateAgentKeyRequest {
            agent_id: "nebu".to_string(),
            name: format!("nebu-{}", uuid::Uuid::new_v4()),
            duration: 604800,
        };

        let agent_key = create_agent_key(
            &config.auth_server.unwrap(),
            &user_profile.token.clone().unwrap(),
            create_agent_key_request,
        )
        .await
        .unwrap();

        Ok(agent_key.key.unwrap())
    }

    /// Store an agent key as a secret in the database
    async fn store_agent_key_secret(
        &self,
        db: &DatabaseConnection,
        user_profile: &V1UserProfile,
        container_id: &str,
        owner_id: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        use crate::entities::secrets;
        use sea_orm::{EntityTrait, Set};

        let agent_key = self.get_agent_key(user_profile).await?;

        // Create a new secret with the agent key
        let secret = secrets::Model::new(
            container_id.to_string(),
            format!("agent-key-{}", container_id),
            "container-reconciler".to_string(),
            &agent_key,
            Some(owner_id.to_string()),
            None,
        )?;

        // Convert to active model for insertion
        let active_model = secrets::ActiveModel {
            id: Set(secret.id),
            name: Set(secret.name),
            owner_id: Set(secret.owner_id),
            encrypted_value: Set(secret.encrypted_value),
            nonce: Set(secret.nonce),
            labels: Set(None),
            created_by: Set(secret.created_by),
            updated_at: Set(secret.updated_at),
            created_at: Set(secret.created_at),
        };

        // Insert into database
        secrets::Entity::insert(active_model)
            .exec(db)
            .await
            .map_err(|e| {
                Box::<dyn std::error::Error + Send + Sync>::from(format!(
                    "Failed to store agent key secret: {}",
                    e
                ))
            })?;

        info!(
            "[RunPod] Stored agent key secret for container {}",
            container_id
        );

        Ok(())
    }
}

pub trait ContainerController {
    async fn reconcile(&self);
}
