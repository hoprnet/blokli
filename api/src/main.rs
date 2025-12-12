//! Main entry point for the blokli API server

use std::{path::PathBuf, sync::Arc};

use blokli_api::schema::export_schema_sdl;
use blokli_chain_api::{
    rpc_adapter::RpcAdapter,
    transaction_executor::{RawTransactionExecutor, RawTransactionExecutorConfig},
    transaction_store::TransactionStore,
    transaction_validator::TransactionValidator,
};
use blokli_chain_indexer::IndexerState;
use blokli_chain_rpc::{
    client::DefaultRetryPolicy,
    rpc::{RpcOperations, RpcOperationsConfig},
    transport::ReqwestClient,
};
use blokli_chain_types::ContractAddresses;
use clap::{Parser, Subcommand};
use hopr_primitive_types::primitives::Address;
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

                // Create a default IndexerState for schema export
                // This is only used for schema generation, not for actual indexing
                // Use minimal buffer sizes since no events will flow through
                let indexer_state = IndexerState::new(1, 1);

                // Create stub transaction components for schema export
                let transaction_store = Arc::new(TransactionStore::new());
                let transaction_validator = Arc::new(TransactionValidator::new());

                // Create a minimal RPC connection for stub purposes
                let transport_client = alloy::transports::http::ReqwestTransport::new(
                    url::Url::parse("http://localhost:8545").expect("Failed to parse stub RPC URL"),
                );
                let rpc_client = alloy::rpc::client::ClientBuilder::default()
                    .layer(alloy::transports::layers::RetryBackoffLayer::new_with_policy(
                        2,
                        100,
                        100,
                        DefaultRetryPolicy::default(),
                    ))
                    .transport(transport_client.clone(), transport_client.guess_local());

                // Use default chain ID for schema export (Gnosis Chain)
                let chain_id = 100u64;

                let rpc_operations = RpcOperations::new(
                    rpc_client.clone(),
                    ReqwestClient::new(),
                    RpcOperationsConfig {
                        chain_id,
                        contract_addrs: ContractAddresses {
                            token: Address::default(),
                            channels: Address::default(),
                            announcements: Address::default(),
                            module_implementation: Address::default(),
                            node_safe_migration: Address::default(),
                            node_safe_registry: Address::default(),
                            ticket_price_oracle: Address::default(),
                            winning_probability_oracle: Address::default(),
                            node_stake_v2_factory: Address::default(),
                        },
                        ..Default::default()
                    },
                    None,
                )
                .expect("Failed to create stub RPC operations");

                let rpc_adapter = Arc::new(RpcAdapter::new(rpc_operations.clone()));

                let transaction_executor = Arc::new(RawTransactionExecutor::with_shared_dependencies(
                    rpc_adapter,
                    transaction_store.clone(),
                    transaction_validator,
                    RawTransactionExecutorConfig::default(),
                ));

                // Generate schema SDL
                let schema_sdl = export_schema_sdl(
                    db,
                    chain_id,
                    ContractAddresses::default(),
                    indexer_state,
                    transaction_executor,
                    transaction_store,
                    Arc::new(rpc_operations),
                    None, // No SQLite notification manager for schema export
                );

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
