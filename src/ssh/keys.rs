use std::fs;
use std::io::Result;
use std::process::Command;
use tempfile::tempdir;

pub fn generate_ssh_keypair() -> Result<(String, String)> {
    // Create a temporary directory
    let temp_dir = tempdir()?;
    let key_path = temp_dir.path().join("id_rsa");
    let key_path_str = key_path.to_string_lossy().to_string();

    let output = Command::new("ssh-keygen")
        .args(&["-t", "rsa", "-b", "2048", "-f", &key_path_str, "-N", ""])
        .output()?;

    if output.status.success() {
        println!("SSH key pair generated successfully.");

        // Read the private and public key files
        let private_key = fs::read_to_string(&key_path)?;
        let public_key = fs::read_to_string(format!("{}.pub", &key_path_str))?;

        // The temporary directory and its contents will be automatically
        // removed when `temp_dir` goes out of scope.
        Ok((private_key, public_key))
    } else {
        eprintln!("Error: {}", String::from_utf8_lossy(&output.stderr));
        // Return an error if SSH key generation failed
        Err(std::io::Error::new(
            std::io::ErrorKind::Other,
            "SSH key generation failed",
        ))
    }
}
