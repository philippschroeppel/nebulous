use crate::entities::containers;
use crate::models::{V1Container, V1ContainerRequest, V1UserProfile};
use sea_orm::DatabaseConnection;
use std::fmt;
use std::str::FromStr;

/// Enum for container status
#[derive(Debug, serde::Serialize, serde::Deserialize, Clone, PartialEq)]
pub enum ProcessorStatus {
    Defined,
    Scaling,
    Pending,
    Running,
    Failed,
    Stopped,
    Invalid,
    Creating,
    Created,
}

impl ProcessorStatus {
    /// Returns a list of all statuses considered inactive (terminal).
    pub fn inactive() -> Vec<Self> {
        vec![
            ProcessorStatus::Failed,
            ProcessorStatus::Stopped,
            ProcessorStatus::Invalid,
        ]
    }

    /// Returns a list of all statuses considered active (non-terminal).
    pub fn active() -> Vec<Self> {
        vec![
            ProcessorStatus::Defined,
            ProcessorStatus::Scaling,
            ProcessorStatus::Pending,
            ProcessorStatus::Running,
            ProcessorStatus::Creating,
            ProcessorStatus::Created,
        ]
    }

    pub fn needs_start(&self) -> bool {
        matches!(
            self,
            ProcessorStatus::Defined | ProcessorStatus::Scaling | ProcessorStatus::Pending
        )
    }

    pub fn needs_watch(&self) -> bool {
        matches!(
            self,
            ProcessorStatus::Running | ProcessorStatus::Creating | ProcessorStatus::Created
        )
    }

    /// Returns true if the container is in a terminal (inactive) state.
    pub fn is_inactive(&self) -> bool {
        matches!(
            self,
            ProcessorStatus::Failed | ProcessorStatus::Stopped | ProcessorStatus::Invalid
        )
    }

    /// Returns true if the container is in an active (non-terminal) state.
    pub fn is_active(&self) -> bool {
        !self.is_inactive()
    }
}

impl fmt::Display for ProcessorStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ProcessorStatus::Defined => write!(f, "defined"),
            ProcessorStatus::Scaling => write!(f, "scaling"),
            ProcessorStatus::Pending => write!(f, "pending"),
            ProcessorStatus::Running => write!(f, "running"),
            ProcessorStatus::Failed => write!(f, "failed"),
            ProcessorStatus::Stopped => write!(f, "stopped"),
            ProcessorStatus::Invalid => write!(f, "invalid"),
            ProcessorStatus::Creating => write!(f, "creating"),
            ProcessorStatus::Created => write!(f, "created"),
        }
    }
}

impl FromStr for ProcessorStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "defined" => Ok(ProcessorStatus::Defined),
            "scaling" => Ok(ProcessorStatus::Scaling),
            "pending" => Ok(ProcessorStatus::Pending),
            "running" => Ok(ProcessorStatus::Running),
            "failed" => Ok(ProcessorStatus::Failed),
            "stopped" => Ok(ProcessorStatus::Stopped),
            "creating" => Ok(ProcessorStatus::Creating),
            "created" => Ok(ProcessorStatus::Created),
            _ => Err(format!("Unknown processor status: {}", s)),
        }
    }
}

pub trait ProcessorPlatform {
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

    async fn delete(
        &self,
        id: &str,
        db: &DatabaseConnection,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>>;
}

pub trait ContainerController {
    async fn reconcile(&self);
}
