mod queries;
mod subscriptions;

use std::str::FromStr;
use std::time::Duration;

use crate::subscriptions::{ChannelAllowedStates, SubscriptionTarget};
use blokli_client::api::types::Account;
use blokli_client::{
    BlokliClient, BlokliClientConfig,
    api::{AccountSelector, BlokliTransactionClient, ChannelFilter, ChannelSelector, types::ChannelStatus},
};
use clap::{Args, Parser, Subcommand, ValueEnum};
use futures::{StreamExt, TryFuture, TryFutureExt, future::Either, pin_mut};
use hopr_crypto_types::types::OffchainPublicKey;
use hopr_primitive_types::prelude::{Address, ToHex};
use queries::QueryTarget;
use tokio::io::AsyncReadExt;
use tracing_subscriber::{EnvFilter, fmt};

#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Cli {
    /// URL of Blokli instance connect to (e.g., http://localhost:8080).
    #[arg(short, long, env = "BLOKLI_URL", value_parser = clap::value_parser!(url::Url))]
    url: url::Url,
    /// Output format.
    #[arg(short, long, env, value_enum, default_value = "json")]
    format: Formats,

    #[clap(subcommand)]
    command: Commands,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum Formats {
    /// Output in JSON format.
    Json,
    /// Output in YAML format.
    Yaml,
}

impl Formats {
    pub fn serialize<T: serde::Serialize>(&self, value: T) -> anyhow::Result<String> {
        match self {
            Formats::Json => Ok(serde_json::to_string_pretty(&value)?),
            Formats::Yaml => Ok(serde_yaml::to_string(&value)?),
        }
    }
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Perform a query to Blokli.
    #[clap(visible_alias = "q")]
    Query {
        #[clap(subcommand)]
        target: QueryTarget,
    },
    /// Subscribe to events from Blokli.
    #[clap(visible_alias = "sub")]
    Subscribe {
        #[clap(subcommand)]
        target: SubscriptionTarget,
    },
    /// Submit an on-chain transaction using Blokli.
    #[clap(visible_alias = "tx")]
    Transaction {
        /// Hex-encoded transaction payload.
        ///
        /// If not specified, reads the payload from the standard input as raw bytes.
        #[arg(short, long)]
        payload: Option<String>,
        /// Number of blocks to wait for confirmation.
        #[arg(short, long, group = "tx")]
        wait_for_confirmation: Option<usize>,
        /// Indicates whether to track the transaction status instead of waiting for confirmations.
        #[arg(short, long, group = "tx")]
        track: bool,
    },
}

#[derive(Debug, Clone, Args)]
pub(crate) struct ChannelArgs {
    /// Channels with the given source key id.
    #[arg(short, long)]
    src_key_id: Option<u32>,
    /// Channels with the given destination key id.
    #[arg(short, long)]
    dst_key_id: Option<u32>,
    /// Channels with the given status.
    #[arg(short, long, value_enum)]
    allowed_states: Option<ChannelAllowedStates>,
    /// Channel with the given ID.
    #[arg(short, long)]
    channel_id: Option<String>,
}

impl TryFrom<ChannelArgs> for ChannelSelector {
    type Error = anyhow::Error;

    fn try_from(value: ChannelArgs) -> Result<Self, Self::Error> {
        let ChannelArgs {
            src_key_id,
            dst_key_id,
            channel_id,
            allowed_states,
        } = value;
        Ok(ChannelSelector {
            filter: match (src_key_id, dst_key_id, channel_id) {
                (Some(src), None, None) => Some(ChannelFilter::SourceKeyId(src)),
                (None, Some(dst), None) => Some(ChannelFilter::DestinationKeyId(dst)),
                (Some(src), Some(dst), None) => Some(ChannelFilter::SourceAndDestinationKeyIds(src, dst)),
                (None, None, Some(channel_id)) => {
                    let channel_id = channel_id.to_lowercase();
                    Some(ChannelFilter::ChannelId(
                        hex::decode(channel_id.trim_start_matches("0x"))?
                            .try_into()
                            .map_err(|_| anyhow::anyhow!("invalid channel ID"))?,
                    ))
                }
                (None, None, None) => None,
                _ => {
                    return Err(anyhow::anyhow!(
                        "Invalid combination of --src-key-id, --dst-key-id and --channel-id given."
                    ));
                }
            },
            status: allowed_states.map(|s| match s {
                ChannelAllowedStates::Open => ChannelStatus::Open,
                ChannelAllowedStates::PendingToClose => ChannelStatus::PendingToClose,
                ChannelAllowedStates::Closed => ChannelStatus::Closed,
            }),
        })
    }
}

#[derive(Debug, Clone, Args)]
pub(crate) struct AccountArgs {
    /// Account chain address.
    #[arg(short, long, value_parser = clap::value_parser!(Address), group = "selector")]
    address: Option<Address>,
    /// Account packet key (either in hex or as a Peer ID).
    #[arg(short, long, group = "selector")]
    packet_key: Option<String>,
    /// Account key id.
    #[arg(short, long, group = "selector")]
    key_id: Option<u32>,
    /// Show peer IDs for accounts.
    #[arg(short, long)]
    show_peer_ids: bool,
}

impl TryFrom<AccountArgs> for AccountSelector {
    type Error = anyhow::Error;

    fn try_from(value: AccountArgs) -> Result<Self, Self::Error> {
        let AccountArgs {
            address,
            key_id,
            packet_key,
            ..
        } = value;
        match (address, key_id, packet_key) {
            (Some(address), None, None) => Ok(AccountSelector::Address(address.into())),
            (None, Some(key_id), None) => Ok(AccountSelector::KeyId(key_id)),
            (None, None, Some(packet_key)) => {
                if let Ok(key) = OffchainPublicKey::from_hex(&packet_key) {
                    eprintln!("Corresponding PeerId: {}", key.to_peerid_str());
                    Ok(AccountSelector::PacketKey(key.into()))
                } else if let Ok(key) = OffchainPublicKey::from_peerid(&packet_key.parse()?) {
                    eprintln!("Corresponding packet key: {}", key.to_hex());
                    Ok(AccountSelector::PacketKey(key.into()))
                } else {
                    Err(anyhow::anyhow!("Cannot parse packet key: {packet_key}"))
                }
            }
            (None, None, None) => Ok(AccountSelector::Any),
            _ => Err(anyhow::anyhow!("Cannot specify both --address and --key-id.")),
        }
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct AltAccount {
    pub chain_key: String,
    pub keyid: i32,
    pub multi_addresses: Vec<String>,
    pub packet_key: String,
    pub peer_id: String,
    pub safe_address: Option<String>,
}

impl TryFrom<Account> for AltAccount {
    type Error = anyhow::Error;

    fn try_from(value: Account) -> Result<Self, Self::Error> {
        Ok(Self {
            chain_key: value.chain_key,
            keyid: value.keyid,
            multi_addresses: value.multi_addresses,
            peer_id: OffchainPublicKey::from_str(&value.packet_key)?.to_peerid_str(),
            packet_key: value.packet_key,
            safe_address: value.safe_address,
        })
    }
}

fn either_err<A, B>(either: Either<(<A as TryFuture>::Error, B), (<B as TryFuture>::Error, A)>) -> anyhow::Error
where
    A: TryFuture,
    B: TryFuture,
    A::Error: Into<anyhow::Error>,
    B::Error: Into<anyhow::Error>,
{
    match either {
        Either::Left((e, _)) => e.into(),
        Either::Right((e, _)) => e.into(),
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .with_level(true)
        .compact()
        .init();

    let cli = Cli::parse();
    let blokli_client = BlokliClient::new(cli.url, BlokliClientConfig::default());

    let exit_fut = tokio::signal::ctrl_c().inspect_ok(|_| {
        eprintln!("\nInterrupted.");
    });
    pin_mut!(exit_fut);

    match cli.command {
        Commands::Query { target } => {
            let exec_fut = target.execute(&blokli_client, cli.format);
            pin_mut!(exec_fut);

            if let Either::Right((value, _)) = futures::future::try_select(exit_fut, exec_fut)
                .map_err(either_err)
                .await?
            {
                println!("{value}");
            }
        }
        Commands::Subscribe { target } => {
            let stream_fut = target.execute(&blokli_client, cli.format)?.for_each(|v| {
                println!("{v}");
                futures::future::ready(())
            });
            pin_mut!(stream_fut);

            futures::future::select(exit_fut, stream_fut).await;
        }
        Commands::Transaction {
            payload,
            wait_for_confirmation,
            track,
        } => {
            let payload = if let Some(payload) = payload {
                let payload = payload.to_lowercase();
                hex::decode(payload.trim_start_matches("0x"))?
            } else {
                eprintln!("Waiting for transaction payload from stdin...");
                let mut payload = Vec::new();
                tokio::io::stdin().read_to_end(&mut payload).await?;
                payload
            };

            if let Some(confirmations) = wait_for_confirmation {
                let tx_fut = blokli_client.submit_and_confirm_transaction(&payload, confirmations);
                pin_mut!(tx_fut);

                if let Either::Right((receipt, _)) = futures::future::try_select(exit_fut, tx_fut)
                    .map_err(either_err)
                    .await?
                {
                    println!("{}", hex::encode(receipt));
                }
            } else if track {
                let track_tx_fut = tokio::time::timeout(
                    Duration::from_secs(10),
                    blokli_client.submit_and_track_transaction(&payload),
                )
                .map_err(anyhow::Error::from)
                .and_then(|res| futures::future::ready(res.map_err(anyhow::Error::from)))
                .inspect_ok(|tx_id| eprintln!("{tx_id}"))
                .and_then(|tracking_id| {
                    blokli_client
                        .track_transaction(tracking_id, Duration::from_secs(60))
                        .map_err(anyhow::Error::from)
                });
                pin_mut!(track_tx_fut);

                if let Either::Right((transaction, _)) = futures::future::try_select(exit_fut, track_tx_fut)
                    .map_err(either_err)
                    .await?
                {
                    println!("{}", cli.format.serialize(transaction)?)
                }
            } else {
                let tx_fut = blokli_client.submit_transaction(&payload);
                pin_mut!(tx_fut);

                if let Either::Right((receipt, _)) = futures::future::try_select(exit_fut, tx_fut)
                    .map_err(either_err)
                    .await?
                {
                    println!("{}", hex::encode(receipt));
                }
            }
        }
    };

    Ok(())
}
