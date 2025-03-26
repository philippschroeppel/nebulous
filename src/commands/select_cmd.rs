use nebulous::select::checkpoint::select_checkpoint;
use std::error::Error;
use std::path::Path; // Path to your select.rs
                     // or "use select::select_checkpoint;" depending on how you import it.

pub fn execute(base_dir: String, criteria: String) -> Result<(), Box<dyn Error>> {
    let result = select_checkpoint(Path::new(&base_dir), &criteria)?;
    match result {
        Some(path) => {
            println!("Selected checkpoint: {}", path.display());
        }
        None => {
            println!("No matching checkpoint found.");
        }
    }

    Ok(())
}
