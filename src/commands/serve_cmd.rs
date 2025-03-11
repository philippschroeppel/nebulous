// src/commands/serve.rs

use nebulous::create_app;
use std::error::Error;

pub async fn execute(host: String, port: u16) -> Result<(), Box<dyn Error>> {
    let app = create_app().await?;
    // Run it
    let addr = format!("{}:{}", host, port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    println!("Server running at http://{}", addr);
    axum::serve(listener, app).await?;

    Ok(())
}
