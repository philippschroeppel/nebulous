use oci_distribution::client::Client;
use oci_distribution::manifest::{OciImageIndex, OciImageManifest, OciManifest};
use oci_distribution::secrets::RegistryAuth;
use oci_distribution::Reference;
use serde_json::Value;
use tracing::debug;

// Example of manually pulling an image from a multi-arch index
// *without* specifying architecture or OS
pub async fn pull_and_parse_config(
    image_ref: &str,
) -> Result<(OciImageManifest, String), Box<dyn std::error::Error + Send + Sync>> {
    // 1. Create the OCI client (no “pull options” or “ClientConfig” needed)
    let client = Client::default();
    let reference: Reference = image_ref.parse()?;

    // 2. Authenticate if needed (Anonymous here)
    client
        .auth(
            &reference,
            &RegistryAuth::Anonymous,
            oci_distribution::RegistryOperation::Pull,
        )
        .await?;

    // 3. Pull the manifest (could be `Image` or `Index`)
    let (manifest_enum, top_digest) = client
        // `_pull_manifest` is the internal function. But for public usage,
        // you can use `pull_manifest` and then match on the returned `OciManifest`.
        .pull_manifest(&reference, &RegistryAuth::Anonymous)
        .await?;

    // 4. If it’s an image index -> pick a sub-manifest yourself
    let pinned_reference = match manifest_enum {
        OciManifest::Image(_image_manifest) => {
            debug!("Got a single-arch image manifest, digest={}", top_digest);
            // Already pinned if you want, or just keep by-tag reference
            // If you want the pinned form: <repo>@sha256:<digest>:
            Reference::with_digest(
                reference.registry().to_string(),
                reference.repository().to_string(),
                top_digest,
            )
        }
        OciManifest::ImageIndex(OciImageIndex { manifests, .. }) => {
            if let Some(first_entry) = manifests.first() {
                debug!(
                    "Got a multi-arch index with {} entries. Picking digest={}",
                    manifests.len(),
                    first_entry.digest
                );
                // Construct pinned reference from the chosen sub-manifest
                Reference::with_digest(
                    reference.registry().to_string(),
                    reference.repository().to_string(),
                    first_entry.digest.clone(),
                )
            } else {
                return Err("No sub-manifests found in multi-arch index".into());
            }
        }
    };

    // 5. Now pull again (this time we expect a single `Image`).
    let (manifest_enum2, pinned_digest) = client
        .pull_manifest(&pinned_reference, &RegistryAuth::Anonymous)
        .await?;

    let image_manifest = match manifest_enum2 {
        OciManifest::Image(img) => {
            debug!("Sub-manifest pin succeeded, digest={}", pinned_digest);
            img
        }
        OciManifest::ImageIndex(_) => {
            return Err("Still got an index?! No valid single-arch manifest found.".into());
        }
    };

    // 6. Pull the config blob from that pinned reference
    let mut config_bytes = Vec::new();
    debug!("Pulling config layer from pinned reference...");
    client
        .pull_blob(&pinned_reference, &image_manifest.config, &mut config_bytes)
        .await?;

    // 7. Convert config to string, parse JSON
    let config_str = String::from_utf8(config_bytes)?;
    debug!("Config JSON:\n{}", config_str);
    let config_json: Value = serde_json::from_str(&config_str)?;

    // Example: extract the `User` field from config
    let user = config_json
        .get("config")
        .and_then(|c| c.get("User"))
        .and_then(|u| u.as_str())
        .unwrap_or("root");

    debug!("Found user={user}");
    Ok((image_manifest, user.to_owned()))
}
