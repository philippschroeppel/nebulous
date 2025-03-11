use crate::models::{Container, ContainerRequest};
use sea_orm::DatabaseConnection;
use std::collections::HashMap;
use std::env;

pub trait ContainerPlatform {
    fn run(
        &self,
        config: &ContainerRequest,
        db: &DatabaseConnection,
        owner_id: &str,
    ) -> Result<Container, Box<dyn std::error::Error>>;

    fn delete(&self, id: &str, db: &DatabaseConnection) -> Result<(), Box<dyn std::error::Error>>;

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
