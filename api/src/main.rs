//! Main entry point for the blokli API server

use std::path::PathBuf;

use blokli_api::schema::export_schema_sdl;
use blokli_chain_types::ContractAddresses;
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

                // Generate schema SDL (using default contract addresses for schema export)
                let schema_sdl = export_schema_sdl(db, ContractAddresses::default());

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

    // The API server is not meant to be run standalone
    // It should only be run as part of bloklid
    eprintln!("Error: blokli-api is not meant to be run standalone.");
    eprintln!("Please run 'bloklid' instead, which embeds the API server.");
    eprintln!();
    eprintln!("To export the GraphQL schema, use:");
    eprintln!("  blokli-api export-schema -d <database-url> -o schema.graphql");
    std::process::exit(1);
}
