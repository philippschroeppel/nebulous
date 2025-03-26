use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// Select a checkpoint directory by either "latest" (numerically highest)
/// or "best" (highest eval score).
///
/// - base_dir: Base directory containing "checkpoint-N" subdirectories.
/// - criteria: Either "latest" or "best".
///
/// Returns the path of the selected checkpoint directory, or None if none found.
pub fn select_checkpoint(base_dir: &Path, criteria: &str) -> io::Result<Option<PathBuf>> {
    // Read all directories inside base_dir.
    let entries = fs::read_dir(base_dir)?;

    let mut best_path: Option<PathBuf> = None;

    // For "latest" we track the highest checkpoint number.
    let mut best_checkpoint_num: i64 = -1;

    // For "best" (lowest eval loss), we start with a large sentinel value.
    let mut best_loss: f64 = f64::MAX;

    for entry in entries {
        let entry = entry?;
        let path = entry.path();

        // Only proceed if it's a directory named "checkpoint-<number>".
        if path.is_dir() {
            if let Some(dir_name) = path.file_name().and_then(|n| n.to_str()) {
                if let Some(checkpoint_num_str) = dir_name.strip_prefix("checkpoint-") {
                    // Attempt to parse the trailing number.
                    if let Ok(checkpoint_num) = checkpoint_num_str.parse::<i64>() {
                        match criteria {
                            "latest" => {
                                // We pick the largest checkpoint number.
                                if checkpoint_num > best_checkpoint_num {
                                    best_checkpoint_num = checkpoint_num;
                                    best_path = Some(path.clone());
                                }
                            }
                            "best" => {
                                // Look for Hugging Face `trainer_state.json`.
                                let trainer_state_file = path.join("trainer_state.json");

                                // Default to "very large" if we can't read or parse.
                                let eval_loss = match fs::read_to_string(&trainer_state_file) {
                                    Ok(contents) => {
                                        match serde_json::from_str::<serde_json::Value>(&contents) {
                                            Ok(json_val) => json_val
                                                .get("metrics")
                                                .and_then(|metrics| metrics.get("eval_loss"))
                                                .and_then(|val| val.as_f64())
                                                .unwrap_or(f64::MAX),
                                            Err(_) => f64::MAX,
                                        }
                                    }
                                    Err(_) => f64::MAX,
                                };

                                // Update the "best checkpoint" when we find a new lowest eval_loss.
                                if eval_loss < best_loss {
                                    best_loss = eval_loss;
                                    best_path = Some(path.clone());
                                }
                            }
                            _ => {
                                // Unrecognized criterion, do nothing or handle as an error.
                            }
                        }
                    }
                }
            }
        }
    }

    Ok(best_path)
}
