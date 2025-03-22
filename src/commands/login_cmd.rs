use std::error::Error;
use std::io::{self, Write};

use nebulous::config::GlobalConfig;
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
    config.api_key = Some(api_key);
    config.server = Some("https://nebu.agentlabs.xyz".to_string());
    config.write()?;

    println!("\nLogin successful!");
    Ok(())
}
