mod queries;
mod subscriptions;

use blokli_client::{
    BlokliClient, BlokliClientConfig,
    api::{AccountSelector, ChannelFilter, ChannelSelector, types::ChannelStatus},
};
use clap::{Args, Parser, Subcommand, ValueEnum};
use futures::{StreamExt, TryFutureExt, pin_mut};
use hopr_primitive_types::prelude::Address;
use queries::QueryTarget;
use tracing_subscriber::{EnvFilter, fmt};

use crate::subscriptions::{ChannelAllowedStates, SubscriptionTarget};

#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Cli {
    /// URL of Blokli instance connect to.
    #[arg(short, long, value_parser = clap::value_parser!(url::Url))]
    url: url::Url,

    #[arg(short, long, value_enum, default_value = "json")]
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
    Query {
        #[clap(subcommand)]
        target: QueryTarget,
    },
    /// Subscribe to events from Blokli.
    Subscribe {
        #[clap(subcommand)]
        target: SubscriptionTarget,
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
                (Some(src), None, None) => ChannelFilter::SourceKeyId(src),
                (None, Some(dst), None) => ChannelFilter::DestinationKeyId(dst),
                (Some(src), Some(dst), None) => ChannelFilter::SourceAndDestinationKeyIds(src, dst),
                (None, None, Some(channel_id)) => ChannelFilter::ChannelId(
                    hex::decode(channel_id)?
                        .try_into()
                        .map_err(|_| anyhow::anyhow!("invalid channel ID"))?,
                ),
                _ => {
                    return Err(anyhow::anyhow!(
                        "At least one of --src-key-id or --dst-key-id must be specified, or a --channel-id \
                         given."
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
    /// Account key id.
    #[arg(short, long, group = "selector")]
    key_id: Option<u32>,
}

impl TryFrom<AccountArgs> for AccountSelector {
    type Error = anyhow::Error;

    fn try_from(value: AccountArgs) -> Result<Self, Self::Error> {
        let AccountArgs { address, key_id } = value;
        match (address, key_id) {
            (Some(address), None) => Ok(AccountSelector::Address(address.into())),
            (None, Some(key_id)) => Ok(AccountSelector::KeyId(key_id)),
            _ => Err(anyhow::anyhow!("Either --address or --key-id must be specified.")),
        }
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .with_level(true)
        .compact()
        .init();

    let cli = Cli::try_parse()?;
    let blokli_client = BlokliClient::new(cli.url, BlokliClientConfig::default());

    match cli.command {
        Commands::Query { target } => {
            println!("{}", target.execute(&blokli_client, cli.format).await?);
        }
        Commands::Subscribe { target } => {
            let stream_fut = target.execute(&blokli_client, cli.format)?.for_each(|v| {
                println!("{v}");
                futures::future::ready(())
            });
            let exit_fut = tokio::signal::ctrl_c().inspect_ok(|_| {
                println!("\nExiting...");
            });
            pin_mut!(exit_fut);
            pin_mut!(stream_fut);

            futures::future::select(exit_fut, stream_fut).await;
        }
    };

    Ok(())
}
