//! Main entry point for the blokli API server

use std::path::PathBuf;

use blokli_api::{config::ApiConfig, schema::export_schema_sdl, start_server};
use clap::{Parser, Subcommand};
use sea_orm::Database;

/// blokli-api: GraphQL API server for HOPR blokli indexer
#[derive(Debug, Parser)]
#[command(name = "blokli-api", about = "GraphQL API server for HOPR blokli indexer", version)]
struct Args {
    /// Optional path to a configuration file
    #[arg(short = 'c', long = "config", value_name = "FILE", global = true)]
    config: Option<PathBuf>,

    /// Command to execute
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Export GraphQL schema to a file in SDL format
    ExportSchema {
        /// Database URL to connect to (required for schema generation)
        #[arg(short = 'd', long = "database-url", env = "DATABASE_URL")]
        database_url: String,

        /// Output file path (defaults to stdout if not specified)
        #[arg(short = 'o', long = "output", value_name = "FILE")]
        output: Option<PathBuf>,
    },
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    // Handle subcommands
    if let Some(command) = args.command {
        match command {
            Command::ExportSchema { database_url, output } => {
                // Connect to database
                let db = Database::connect(&database_url).await?;

                // Generate schema SDL
                let schema_sdl = export_schema_sdl(db);

                // Write to file or stdout
                if let Some(output_path) = output {
                    std::fs::write(&output_path, schema_sdl)?;
                    eprintln!("GraphQL schema exported to: {}", output_path.display());
                } else {
                    println!("{}", schema_sdl);
                }

                return Ok(());
            }
        }
    }

    // Load configuration (for now using defaults)
    let config = ApiConfig::default();

    // Start the server
    start_server(config).await?;

    Ok(())
}
