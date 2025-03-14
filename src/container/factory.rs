use crate::container::base::ContainerPlatform;
use crate::container::kube::KubePlatform;
use crate::container::runpod::RunpodPlatform;
use crate::entities::containers;
use crate::models::{V1Container, V1ContainerRequest, V1UserProfile};
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
    ) -> Result<V1Container, Box<dyn Error>> {
        match self {
            PlatformType::Runpod(platform) => {
                platform.declare(request, db, user_profile, owner_id).await
            }
            PlatformType::Kube(platform) => {
                platform.declare(request, db, user_profile, owner_id).await
            }
        }
    }

    pub async fn reconcile(
        &self,
        container: &containers::Model,
        db: &DatabaseConnection,
    ) -> Result<(), Box<dyn Error>> {
        match self {
            PlatformType::Runpod(platform) => platform.reconcile(container, db).await,
            PlatformType::Kube(platform) => platform.reconcile(container, db).await,
        }
    }

    pub async fn delete(&self, id: &str, db: &DatabaseConnection) -> Result<(), Box<dyn Error>> {
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
