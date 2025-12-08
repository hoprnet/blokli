use blokli_client::{
    BlokliClient,
    api::{BlokliQueryClient, SafeSelector},
};
use clap::Subcommand;
use hopr_primitive_types::prelude::Address;

use crate::{AccountArgs, ChannelArgs, Formats};

#[derive(Subcommand, Debug)]
pub(crate) enum QueryTarget {
    /// Gets current Blokli version.
    Version,
    /// Gets the health status of Blokli.
    Health,
    /// Gets the native balance of an address.
    NativeBalance {
        #[arg(short, long, value_parser = clap::value_parser!(Address))]
        address: Address,
    },
    /// Gets the token balance of an address.
    TokenBalance {
        #[arg(short, long, value_parser = clap::value_parser!(Address))]
        address: Address,
    },
    /// Gets information about the HOPR on-chain deployment.
    ChainInfo,
    /// Gets information about a Safe.
    Safe {
        /// Safe address.
        #[arg(short, long, value_parser = clap::value_parser!(Address), group = "selector")]
        address: Option<Address>,
        /// Safe owner address.
        #[arg(short, long, value_parser = clap::value_parser!(Address), group = "selector")]
        owner: Option<Address>,
    },
    /// Gets information about an account.
    Account(AccountArgs),
    /// Gets information about channels.
    Channel(ChannelArgs),
}

impl QueryTarget {
    pub(crate) async fn execute(self, client: &BlokliClient, format: Formats) -> anyhow::Result<String> {
        match self {
            QueryTarget::Version => format.serialize(client.query_version().await?),
            QueryTarget::Health => format.serialize(client.query_health().await?),
            QueryTarget::NativeBalance { address } => {
                format.serialize(client.query_native_balance(&address.into()).await?)
            }
            QueryTarget::TokenBalance { address } => {
                format.serialize(client.query_token_balance(&address.into()).await?)
            }
            QueryTarget::ChainInfo => format.serialize(client.query_chain_info().await?),
            QueryTarget::Safe { address, owner } => format.serialize(
                client
                    .query_safe(match (address, owner) {
                        (Some(address), None) => SafeSelector::SafeAddress(address.into()),
                        (None, Some(owner)) => SafeSelector::ChainKey(owner.into()),
                        _ => return Err(anyhow::anyhow!("Either --address or --owner must be specified.")),
                    })
                    .await?,
            ),
            QueryTarget::Account(sel) => format.serialize(client.query_accounts(sel.try_into()?).await?),
            QueryTarget::Channel(sel) => format.serialize(client.query_channels(sel.try_into()?).await?),
        }
    }
}
