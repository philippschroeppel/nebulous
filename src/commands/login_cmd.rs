use std::error::Error;
use std::io::{self, Write};

use nebulous::config::{GlobalConfig, ServerConfig};
use open;
use rpassword;

pub async fn execute(url: String, auth: Option<String>, hub: Option<String>) -> Result<(), Box<dyn Error>> {
    if auth.is_none() ^ hub.is_none() {
        eprintln!("Either auth or hub URL provided. Please provide both.");
        return Ok(());
    }

    let mut config = GlobalConfig::read()?;

    if auth.is_some() && hub.is_some() {
        let auth_address = auth.unwrap();
        let hub_address = hub.unwrap();

        let url = format!("{}/settings/api", hub_address);
        println!("\nVisit {} to get an API key\n", url);

        // Attempt to open the URL in the default browser
        if let Err(e) = open::that(&url) {
            eprintln!("Failed to open browser: {}", e);
        }

        print!("Enter your API key: ");
        io::stdout().flush()?;
        let api_key = rpassword::read_password()?;

        config.servers.push(ServerConfig {
            name: Some("cloud".to_string()),
            server: Some(url),
            api_key: Some(api_key),
            auth_server: Some(auth_address),
        });
        config.current_server = Some("cloud".to_string());
    } else {
        config.servers.push(ServerConfig {
            name: Some("nebu".to_string()),
            server: Some(url),
            api_key: None,
            auth_server: None,
        });
        config.current_server = Some("nebu".to_string());
    }
    config.write()?;

    println!("\nLogin successful!");
    Ok(())
}
