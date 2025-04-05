// src/commands/serve.rs

use nebulous::create_app;
use nebulous::create_app_state;
use nebulous::proxy::server::start_proxy;
use nebulous::resources::v1::containers::controller::ContainerController;
use nebulous::resources::v1::processors::controller::ProcessorController;
use std::error::Error;

pub async fn execute(host: String, port: u16) -> Result<(), Box<dyn Error>> {
    let app_state = create_app_state().await?;
    let app = create_app(app_state.clone()).await;

    println!("Starting container controller");
    let controller = ContainerController::new(std::sync::Arc::new(app_state.clone()));
    controller.spawn_reconciler();
    println!("Container controller started");

    println!("Starting processor controller");
    let processor_controller = ProcessorController::new(std::sync::Arc::new(app_state.clone()));
    processor_controller.spawn_reconciler();
    println!("Processor controller started");

    println!("Starting proxy server");
    tokio::spawn({
        let proxy_state = app_state.clone();
        async move {
            if let Err(e) = start_proxy(proxy_state, 3030).await {
                eprintln!("Error in proxy server: {}", e);
            }
        }
    });
    println!("Proxy server started in background");

    // Run it
    println!("Starting main server");
    let addr = format!("{}:{}", host, port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    println!("Server running at http://{}", addr);
    axum::serve(listener, app).await?;

    Ok(())
}
