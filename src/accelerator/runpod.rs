use std::collections::HashMap;
use std::fs;
use std::path::Path;

use crate::accelerator::base::{AcceleratorProvider, ConfigError, PlatformConfig};

/// RunPod implementation of AcceleratorProvider
pub struct RunPodProvider {
    config: PlatformConfig,
}

impl RunPodProvider {
    /// Create a new RunPod provider with default configuration
    pub fn new() -> Self {
        Self {
            config: PlatformConfig {
                name: "runpod".to_string(),
                accelerator_map: [
                    ("A100_PCIe", "NVIDIA A100 80GB PCIe"),
                    ("A100_SXM", "NVIDIA A100-SXM4-80GB"),
                    ("A30", "NVIDIA A30"),
                    ("A40", "NVIDIA A40"),
                    ("H100_NVL", "NVIDIA H100 NVL"),
                    ("H100_PCIe", "NVIDIA H100 PCIe"),
                    ("H100_SXM", "NVIDIA H100 80GB HBM3"),
                    ("H200_SXM", "NVIDIA H200"),
                    ("L4", "NVIDIA L4"),
                    ("L40", "NVIDIA L40"),
                    ("L40S", "NVIDIA L40S"),
                    ("MI300X", "AMD Instinct MI300X OAM"),
                    ("RTX_2000_Ada", "NVIDIA RTX 2000 Ada Generation"),
                    ("RTX_3070", "NVIDIA GeForce RTX 3070"),
                    ("RTX_3080", "NVIDIA GeForce RTX 3080"),
                    ("RTX_3080_Ti", "NVIDIA GeForce RTX 3080 Ti"),
                    ("RTX_3090", "NVIDIA GeForce RTX 3090"),
                    ("RTX_3090_Ti", "NVIDIA GeForce RTX 3090 Ti"),
                    ("RTX_4000_Ada", "NVIDIA RTX 4000 Ada Generation"),
                    ("RTX_4070_Ti", "NVIDIA GeForce RTX 4070 Ti"),
                    ("RTX_4080", "NVIDIA GeForce RTX 4080"),
                    ("RTX_4080_SUPER", "NVIDIA GeForce RTX 4080 SUPER"),
                    ("RTX_4090", "NVIDIA GeForce RTX 4090"),
                    ("RTX_5000_Ada", "NVIDIA RTX 5000 Ada Generation"),
                    ("RTX_6000_Ada", "NVIDIA RTX 6000 Ada Generation"),
                    ("RTX_A2000", "NVIDIA RTX A2000"),
                    ("RTX_A4000", "NVIDIA RTX A4000"),
                    ("RTX_A4500", "NVIDIA RTX A4500"),
                    ("RTX_A5000", "NVIDIA RTX A5000"),
                    ("RTX_A6000", "NVIDIA RTX A6000"),
                    ("V100", "Tesla V100-PCIE-16GB"),
                    ("V100_FHHL", "Tesla V100-FHHL-16GB"),
                    ("V100_SXM2", "Tesla V100-SXM2-16GB"),
                    ("V100_SXM2_32GB", "Tesla V100-SXM2-32GB"),
                ]
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
            },
        }
    }

    /// Load RunPod configuration from a file
    pub fn from_path<P: AsRef<Path>>(path: P) -> Result<Self, ConfigError> {
        let config_content =
            fs::read_to_string(path).map_err(|e| ConfigError::IoError(e.to_string()))?;

        let config: PlatformConfig = serde_yaml::from_str(&config_content)
            .map_err(|e| ConfigError::ParseError(e.to_string()))?;

        Ok(Self { config })
    }
}

impl AcceleratorProvider for RunPodProvider {
    fn name(&self) -> &str {
        &self.config.name
    }

    fn accelerator_map(&self) -> &HashMap<String, String> {
        &self.config.accelerator_map
    }
}
