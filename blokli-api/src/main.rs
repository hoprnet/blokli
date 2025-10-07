//! Main entry point for the blokli API server

use blokli_api::{config::ApiConfig, start_server};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Load configuration (for now using defaults)
    let config = ApiConfig::default();

    // Start the server
    start_server(config).await?;

    Ok(())
}
