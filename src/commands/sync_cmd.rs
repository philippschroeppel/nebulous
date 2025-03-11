use nebulous::volumes::rclone;
use std::error::Error;

// Example config
//---
// paths:
//   - source_path: "/path/to/local/directory1"
//     destination_path: "s3://your-bucket/directory1"
//   - source_path: "/path/to/local/directory2"
//     destination_path: "s3://your-bucket/directory2"
//   - source_path: "/path/to/local/file.txt"
//     destination_path: "s3://your-bucket/file.txt"

pub async fn execute_sync(
    config_path: String,
    interval_seconds: u64,
    create_if_missing: bool,
    watch: bool,
) -> Result<(), Box<dyn Error>> {
    if watch {
        rclone::execute_continuous_sync(config_path, interval_seconds, create_if_missing).await
    } else {
        rclone::execute_sync(config_path, create_if_missing).await
    }
}
