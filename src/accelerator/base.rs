use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

/// Represents an accelerator with its name and memory capacity in GB
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Accelerator {
    pub name: String,
    pub memory: u32,
}

/// Configuration for accelerators
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AcceleratorsConfig {
    pub supported: Vec<Accelerator>,
}

/// Platform-specific configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlatformConfig {
    pub name: String,
    pub accelerator_map: HashMap<String, String>,
}

/// Root configuration structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub accelerators: AcceleratorsConfig,
}

/// Trait for platform-specific accelerator providers
pub trait AcceleratorProvider {
    /// Get the platform name
    fn name(&self) -> &str;

    /// Get the mapping from internal accelerator names to platform-specific names
    fn accelerator_map(&self) -> &HashMap<String, String>;

    /// Get the platform-specific name for an accelerator
    fn get_platform_name(&self, internal_name: &str) -> Option<&String> {
        self.accelerator_map().get(internal_name)
    }
}

impl Config {
    /// Load configuration from a specified path
    pub fn from_path<P: AsRef<Path>>(path: P) -> Result<Self, ConfigError> {
        let config_content =
            fs::read_to_string(path).map_err(|e| ConfigError::IoError(e.to_string()))?;

        serde_yaml::from_str(&config_content).map_err(|e| ConfigError::ParseError(e.to_string()))
    }

    /// Get an accelerator by its name
    pub fn get_accelerator_by_name(&self, name: &str) -> Option<&Accelerator> {
        self.accelerators
            .supported
            .iter()
            .find(|acc| acc.name == name)
    }

    /// Create a default configuration with predefined accelerators
    pub fn default() -> Self {
        let supported = vec![
            Accelerator {
                name: "A100_PCIe".to_string(),
                memory: 80,
            },
            Accelerator {
                name: "A100_SXM".to_string(),
                memory: 80,
            },
            Accelerator {
                name: "A30".to_string(),
                memory: 24,
            },
            Accelerator {
                name: "A40".to_string(),
                memory: 48,
            },
            Accelerator {
                name: "H100_NVL".to_string(),
                memory: 94,
            },
            Accelerator {
                name: "H100_PCIe".to_string(),
                memory: 80,
            },
            Accelerator {
                name: "H100_SXM".to_string(),
                memory: 80,
            },
            Accelerator {
                name: "H200_SXM".to_string(),
                memory: 143,
            },
            Accelerator {
                name: "L4".to_string(),
                memory: 24,
            },
            Accelerator {
                name: "L40".to_string(),
                memory: 48,
            },
            Accelerator {
                name: "L40S".to_string(),
                memory: 48,
            },
            Accelerator {
                name: "MI300X".to_string(),
                memory: 192,
            },
            Accelerator {
                name: "RTX_2000_Ada".to_string(),
                memory: 16,
            },
            Accelerator {
                name: "RTX_3070".to_string(),
                memory: 8,
            },
            Accelerator {
                name: "RTX_3080".to_string(),
                memory: 10,
            },
            Accelerator {
                name: "RTX_3080_Ti".to_string(),
                memory: 12,
            },
            Accelerator {
                name: "RTX_3090".to_string(),
                memory: 24,
            },
            Accelerator {
                name: "RTX_3090_Ti".to_string(),
                memory: 24,
            },
            Accelerator {
                name: "RTX_4000_Ada".to_string(),
                memory: 20,
            },
            Accelerator {
                name: "RTX_4070_Ti".to_string(),
                memory: 12,
            },
            Accelerator {
                name: "RTX_4080".to_string(),
                memory: 16,
            },
            Accelerator {
                name: "RTX_4080_SUPER".to_string(),
                memory: 16,
            },
            Accelerator {
                name: "RTX_4090".to_string(),
                memory: 24,
            },
            Accelerator {
                name: "RTX_5000_Ada".to_string(),
                memory: 32,
            },
            Accelerator {
                name: "RTX_6000_Ada".to_string(),
                memory: 48,
            },
            Accelerator {
                name: "RTX_A2000".to_string(),
                memory: 6,
            },
            Accelerator {
                name: "RTX_A4000".to_string(),
                memory: 16,
            },
            Accelerator {
                name: "RTX_A4500".to_string(),
                memory: 20,
            },
            Accelerator {
                name: "RTX_A5000".to_string(),
                memory: 24,
            },
            Accelerator {
                name: "RTX_A6000".to_string(),
                memory: 48,
            },
            Accelerator {
                name: "V100".to_string(),
                memory: 16,
            },
            Accelerator {
                name: "V100_FHHL".to_string(),
                memory: 16,
            },
            Accelerator {
                name: "V100_SXM2".to_string(),
                memory: 16,
            },
            Accelerator {
                name: "V100_SXM2_32GB".to_string(),
                memory: 32,
            },
        ];
        Config {
            accelerators: AcceleratorsConfig { supported },
        }
    }
}

/// Error types for configuration operations
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("IO error: {0}")]
    IoError(String),

    #[error("Parse error: {0}")]
    ParseError(String),
}
