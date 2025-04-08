use crate::models::{V1ResourceMeta, V1ResourceMetaRequest};
use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq)]
pub struct V1Volume {
    #[serde(default = "default_volume_kind")]
    pub kind: String,
    pub metadata: V1ResourceMeta,
    pub source: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq)]
pub struct V1VolumeRequest {
    pub metadata: V1ResourceMetaRequest,
    pub source: String,
}

fn default_volume_kind() -> String {
    "Volume".to_string()
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq)]
pub struct V1VolumePath {
    pub source: String,
    pub dest: String,
    #[serde(default)]
    pub resync: bool,
    #[serde(default = "default_continuous")]
    pub continuous: bool,
    #[serde(default = "default_volume_driver")]
    pub driver: V1VolumeDriver,
}

fn default_volume_driver() -> V1VolumeDriver {
    V1VolumeDriver::RCLONE_SYNC
}

fn default_continuous() -> bool {
    false
}

// Add this function to provide a default cache directory
fn default_cache_dir() -> String {
    // Use a sensible default location for the cache
    format!("/nebu/cache")
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq)]
pub struct V1VolumeConfig {
    pub paths: Vec<V1VolumePath>,
    #[serde(default = "default_cache_dir")]
    pub cache_dir: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq)]
pub enum V1VolumeDriver {
    #[default]
    RCLONE_SYNC,
    RCLONE_COPY,
    RCLONE_BISYNC,
    RCLONE_MOUNT,
}

impl std::str::FromStr for V1VolumeDriver {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "RCLONE_BISYNC" => Ok(V1VolumeDriver::RCLONE_BISYNC),
            "RCLONE_SYNC" => Ok(V1VolumeDriver::RCLONE_SYNC),
            "RCLONE_COPY" => Ok(V1VolumeDriver::RCLONE_COPY),
            "RCLONE_MOUNT" => Ok(V1VolumeDriver::RCLONE_MOUNT),
            _ => Err("Unrecognized VolumeType"),
        }
    }
}

impl fmt::Display for V1VolumeDriver {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_string())
    }
}
