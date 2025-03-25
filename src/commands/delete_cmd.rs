use nebulous::config::GlobalConfig;
use reqwest::Client;
use std::error::Error;

pub async fn delete_container(id: String) -> Result<(), Box<dyn Error>> {
    let client = Client::new();
    let config = GlobalConfig::read()?;
    let current_server = config.get_current_server_config().unwrap();
    let server = current_server.server.as_ref().unwrap();
    let api_key = current_server.api_key.as_ref().unwrap();
    let bearer_token = format!("Bearer {}", api_key);

    let url = format!("{}/v1/containers/{}", server, id.trim());

    let response = client
        .delete(&url)
        .header("Authorization", &bearer_token)
        .send()
        .await?;

    if response.status().is_success() {
        println!("Container '{}' successfully deleted", id);
        Ok(())
    } else {
        let error_text = response.text().await?;
        Err(format!("Failed to delete container: {}", error_text).into())
    }
}
