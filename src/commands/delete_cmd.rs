use crate::commands::request::server_request;
use std::error::Error;

pub async fn delete_container(id: String) -> Result<(), Box<dyn Error>> {
    let url = format!("/v1/containers/{}", id.trim());
    let response = server_request(url.as_str(), reqwest::Method::DELETE).await?;

    if response.status().is_success() {
        println!("Container '{}' successfully deleted", id);
        Ok(())
    } else {
        let error_text = response.text().await?;
        Err(format!("Failed to delete container: {}", error_text).into())
    }
}
