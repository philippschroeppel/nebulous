// src/commands/serve.rs

use nebulous::container::controller::ContainerController;
use nebulous::create_app;
use nebulous::create_app_state;
use std::error::Error;

pub async fn execute(host: String, port: u16) -> Result<(), Box<dyn Error>> {
    let app_state = create_app_state().await?;
    let app = create_app(app_state.clone()).await;

    println!("Starting container controller");
    let controller = ContainerController::new(std::sync::Arc::new(app_state));
    controller.spawn_reconciler();
    println!("Container controller started");

    // Run it
    let addr = format!("{}:{}", host, port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    println!("Server running at http://{}", addr);
    axum::serve(listener, app).await?;

    Ok(())
}
