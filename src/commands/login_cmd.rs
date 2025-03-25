use std::error::Error;
use std::io::{self, Write};

use nebulous::config::{GlobalConfig, ServerConfig};
use open;
use rpassword;

pub async fn execute() -> Result<(), Box<dyn Error>> {
    let hub_address = "https://tutor.agentlabs.xyz"; // Use your actual hub address

    let url = format!("{}/settings/api", hub_address);

    println!("\nVisit {} to get an API key\n", url);

    // Attempt to open the URL in the default browser
    if let Err(e) = open::that(&url) {
        eprintln!("Failed to open browser: {}", e);
    }

    // Prompt the user for the API key (input will be hidden)
    print!("Enter your API key: ");
    io::stdout().flush()?;
    let api_key = rpassword::read_password()?;

    // Save the API key to the config file
    let mut config = GlobalConfig::read()?;
    config.servers.push(ServerConfig {
        name: Some("cloud".to_string()),
        server: Some("https://api.nebulous.sh".to_string()),
        api_key: Some(api_key),
        auth_server: Some("https://auth.hub.agentsea.ai".to_string()),
    });
    config.current_server = Some("cloud".to_string());
    config.write()?;

    println!("\nLogin successful!");
    Ok(())
}
