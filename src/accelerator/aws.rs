use std::collections::HashMap;
use std::fs;
use std::path::Path;

use crate::accelerator::base::{AcceleratorProvider, ConfigError, PlatformConfig};

/// AWS implementation of AcceleratorProvider (example)
pub struct AwsProvider {
    config: PlatformConfig,
}

impl AwsProvider {
    /// Create a new AWS provider with default configuration
    pub fn new() -> Self {
        Self {
            config: PlatformConfig {
                name: "aws".to_string(),
                accelerator_map: [
                    ("A100_PCIe", "p4d.24xlarge"),
                    ("A100_SXM", "p4de.24xlarge"),
                    ("H100_PCIe", "p5.48xlarge"),
                    ("H100_SXM", "p5e.48xlarge"),
                ]
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
            },
        }
    }

    /// Load AWS configuration from a file
    pub fn from_path<P: AsRef<Path>>(path: P) -> Result<Self, ConfigError> {
        let config_content =
            fs::read_to_string(path).map_err(|e| ConfigError::IoError(e.to_string()))?;

        let config: PlatformConfig = serde_yaml::from_str(&config_content)
            .map_err(|e| ConfigError::ParseError(e.to_string()))?;

        Ok(Self { config })
    }
}

impl AcceleratorProvider for AwsProvider {
    fn name(&self) -> &str {
        &self.config.name
    }

    fn accelerator_map(&self) -> &HashMap<String, String> {
        &self.config.accelerator_map
    }
}
