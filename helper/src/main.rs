use anyhow::{Context, bail};
use blokli_chain_types::channel::decode_channel;
use clap::{Parser, Subcommand};
use hopr_bindings::exports::alloy::primitives::B256;
use hopr_internal_types::channels::ChannelStatus;

#[derive(Parser)]
#[command(name = "blokli-helper", version, about = "CLI helper utilities for HOPR Blokli")]
struct Cli {
    #[clap(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Decode a packed channel state bytes32 hex string into its constituent fields.
    ///
    /// The channel state is packed into 32 bytes (256 bits) in contract events
    /// with fields: status (u8), epoch (u24), closure_time (u32),
    /// ticket_index (u48), and balance (u96).
    DecodeChannel {
        /// Hex-encoded bytes32 channel state (with or without 0x prefix).
        hex: String,
    },
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::DecodeChannel { hex: hex_str } => {
            let hex_str = hex_str.trim().to_lowercase();
            let hex_str = hex_str.strip_prefix("0x").unwrap_or(&hex_str);

            let bytes = hex::decode(hex_str).context("invalid hex string")?;
            if bytes.len() != 32 {
                bail!("expected 32 bytes (64 hex chars), got {} bytes", bytes.len());
            }

            let mut arr = [0u8; 32];
            arr.copy_from_slice(&bytes);
            let b256 = B256::from(arr);

            let decoded = decode_channel(b256);

            let (status_name, status_byte) = match &decoded.status {
                ChannelStatus::Closed => ("Closed", 0),
                ChannelStatus::Open => ("Open", 1),
                ChannelStatus::PendingToClose(_) => ("PendingToClose", 2),
            };

            println!("Channel State Decoded:");
            println!("  Status:       {status_name} ({status_byte})");
            println!("  Epoch:        {}", decoded.epoch);
            println!("  Closure Time: {}", decoded.closure_time);
            println!(
                "  Ticket Index: {} (0x{:X})",
                decoded.ticket_index, decoded.ticket_index
            );
            println!("  Balance:      {} wei ({})", decoded.balance.amount(), decoded.balance);

            Ok(())
        }
    }
}
