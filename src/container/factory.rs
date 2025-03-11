use crate::container::base::ContainerPlatform;
use crate::container::kube::KubePlatform;
use crate::container::runpod::RunpodPlatform;
use crate::models::{Container, ContainerRequest};
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
    pub async fn run(
        &self,
        request: &ContainerRequest,
        db: &DatabaseConnection,
        owner_id: &str,
    ) -> Result<Container, Box<dyn Error>> {
        match self {
            PlatformType::Runpod(platform) => platform.run(request, db, owner_id),
            PlatformType::Kube(platform) => platform.run(request, db, owner_id),
        }
    }

    pub async fn delete(&self, id: &str, db: &DatabaseConnection) -> Result<(), Box<dyn Error>> {
        match self {
            PlatformType::Runpod(platform) => platform.delete(id, db),
            PlatformType::Kube(platform) => platform.delete(id, db),
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
