use oci_distribution::client::Client;
use oci_distribution::secrets::RegistryAuth;
use oci_distribution::Reference;
use serde_json::Value;
use std::error::Error;
use tracing::debug;

/// Pulls an image's manifest and config from an OCI registry and returns the default container user.
///
/// # Arguments
///
/// * `image_ref` - A string slice that holds the image reference (for example, "docker.io/library/ubuntu:latest").
///
/// # Returns
///
/// A `Result` containing the user as a `String` if successful, or an error.
pub async fn get_container_default_user(
    image_ref: &str,
) -> Result<String, Box<dyn Error + Send + Sync>> {
    debug!("Pulling image manifest and config for {}", image_ref);

    // Create a new OCI client with default settings.
    let mut client = Client::default();

    // Parse the image reference.
    let reference: Reference = image_ref.parse()?;

    debug!("OCI reference: {}", reference);

    // Use anonymous authentication (modify if your registry requires credentials).
    let auth = RegistryAuth::Anonymous;

    // Pull the manifest and configuration.
    // The function returns a tuple: (OciImageManifest, manifest digest, config as String).
    let (_manifest, _digest, config_str) =
        client.pull_manifest_and_config(&reference, &auth).await?;

    debug!("OCI container config str: {}", config_str);

    // Parse the config JSON.
    let config_json: Value = serde_json::from_str(&config_str)?;

    debug!("OCI container config: {}", config_json);

    // Extract the default user from the JSON.
    // This looks in the "config" object for the "User" field.
    // If not found, it defaults to "root".
    let user = config_json
        .get("config")
        .and_then(|cfg| cfg.get("User"))
        .and_then(|user| user.as_str())
        .unwrap_or("root")
        .to_string();

    Ok(user)
}
