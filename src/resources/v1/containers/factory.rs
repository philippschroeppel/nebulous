use crate::entities::containers;
use crate::models::V1UserProfile;
use crate::resources::v1::containers::base::ContainerPlatform;
use crate::resources::v1::containers::kube::KubePlatform;
use crate::resources::v1::containers::models::{V1Container, V1ContainerRequest};
use crate::resources::v1::containers::runpod::RunpodPlatform;
use sea_orm::DatabaseConnection;
use std::error::Error;

// Define an enum that can hold any platform type
pub enum PlatformType {
    Runpod(RunpodPlatform),
    Kube(KubePlatform),
}

// Implement methods on the enum that delegate to the contained platform
impl PlatformType {
    // Example method that both platforms would have
    pub async fn declare(
        &self,
        request: &V1ContainerRequest,
        db: &DatabaseConnection,
        user_profile: &V1UserProfile,
        owner_id: &str,
        namespace: &str,
    ) -> Result<V1Container, Box<dyn Error + Send + Sync>> {
        match self {
            PlatformType::Runpod(platform) => {
                platform
                    .declare(request, db, user_profile, owner_id, namespace)
                    .await
            }
            PlatformType::Kube(platform) => {
                platform
                    .declare(request, db, user_profile, owner_id, namespace)
                    .await
            }
        }
    }

    pub async fn reconcile(
        &self,
        container: &containers::Model,
        db: &DatabaseConnection,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        match self {
            PlatformType::Runpod(platform) => platform.reconcile(container, db).await,
            PlatformType::Kube(platform) => platform.reconcile(container, db).await,
        }
    }

    pub async fn logs(
        &self,
        container_id: &str,
        db: &DatabaseConnection,
    ) -> Result<String, Box<dyn Error + Send + Sync>> {
        match self {
            PlatformType::Runpod(platform) => platform.logs(container_id, db).await,
            PlatformType::Kube(platform) => platform.logs(container_id, db).await,
        }
    }

    pub async fn exec(
        &self,
        container_id: &str,
        command: &str,
        db: &DatabaseConnection,
    ) -> Result<String, Box<dyn Error + Send + Sync>> {
        match self {
            PlatformType::Runpod(platform) => platform.exec(container_id, command, db).await,
            PlatformType::Kube(platform) => platform.exec(container_id, command, db).await,
        }
    }

    pub async fn delete(
        &self,
        id: &str,
        db: &DatabaseConnection,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        match self {
            PlatformType::Runpod(platform) => platform.delete(id, db).await,
            PlatformType::Kube(platform) => platform.delete(id, db).await,
        }
    }

    // Add other methods as needed
}

// Factory function
pub fn platform_factory(platform: String) -> PlatformType {
    match platform.as_str() {
        "runpod" => PlatformType::Runpod(RunpodPlatform::new()),
        "kube" => PlatformType::Kube(KubePlatform::new()),
        _ => panic!("Invalid platform"),
    }
}
