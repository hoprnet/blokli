use blokli_client::{
    BlokliClient,
    api::{BlokliQueryClient, ChainAddress, ModulePredictionInput, SafeSelector},
};
use clap::Subcommand;
use hopr_types::primitive::prelude::Address;

use crate::{AccountArgs, AltAccount, ChannelArgs, Formats, RedemptionsArgs};

#[derive(Subcommand, Debug)]
pub(crate) enum QueryTarget {
    /// Gets current Blokli version.
    Version,
    /// Gets the health status of Blokli.
    Health,
    /// Gets the native balance of an address.
    NativeBalance {
        #[arg(value_parser = clap::value_parser!(Address))]
        address: Address,
    },
    /// Gets the token balance of an address.
    TokenBalance {
        #[arg(value_parser = clap::value_parser!(Address))]
        address: Address,
    },
    /// Gets information about the HOPR on-chain deployment.
    ChainInfo,
    /// Gets the number of transactions sent by an address.
    TxCount {
        #[arg(value_parser = clap::value_parser!(Address))]
        address: Address,
    },
    /// Gets the wxHOPR allowance granted by a Safe to the HOPR channels contract.
    SafeAllowance {
        #[arg(value_parser = clap::value_parser!(Address))]
        address: Address,
    },
    /// Gets information about a Safe.
    Safe {
        /// Safe address.
        #[arg(short, long, value_parser = clap::value_parser!(Address), group = "selector")]
        address: Option<Address>,
        /// Safe owner address.
        #[arg(short, long, value_parser = clap::value_parser!(Address), group = "selector")]
        owner: Option<Address>,
        /// Registered node address.
        #[arg(short, long, value_parser = clap::value_parser!(Address), group = "selector")]
        registered_node: Option<Address>,
    },
    /// Gets the module address prediction.
    ModuleAddress {
        /// Nonce of the Safe deployment.
        #[arg(short, long)]
        nonce: u64,
        /// Safe owner address.
        #[arg(short, long, value_parser = clap::value_parser!(Address), group = "selector")]
        owner: Address,
        /// Predicted Safe address.
        #[arg(short, long, value_parser = clap::value_parser!(Address), group = "selector")]
        safe_address: Address,
    },
    /// Gets the number of accounts.
    CountAccounts(AccountArgs),
    /// Gets information about an account.
    Account(AccountArgs),
    /// Gets the number of channels.
    CountChannels(ChannelArgs),
    /// Gets channels with their aggregated wxHOPR balance. Use --safe-address to scope to a safe.
    Channel(ChannelArgs),
    /// Gets total wxHOPR balance held across indexed safe contracts.
    SafesBalance {
        /// Restrict to safes associated with the given chain key (owner address).
        #[arg(long, value_parser = clap::value_parser!(Address))]
        owner_address: Option<Address>,
    },
    /// Gets information about redemptions
    Redemptions(RedemptionsArgs),
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
            QueryTarget::Safe {
                address,
                owner,
                registered_node,
            } => format.serialize(
                client
                    .query_safe(match (address, owner, registered_node) {
                        (Some(address), None, None) => SafeSelector::SafeAddress(address.into()),
                        (None, Some(owner), None) => SafeSelector::ChainKey(owner.into()),
                        (None, None, Some(registered_node)) => SafeSelector::RegisteredNode(registered_node.into()),
                        _ => {
                            return Err(anyhow::anyhow!(
                                "Exactly one of --address, --owner or --registered-node must be specified."
                            ));
                        }
                    })
                    .await?,
            ),
            QueryTarget::Account(sel) => {
                if sel.show_peer_ids {
                    format.serialize(
                        client
                            .query_accounts(sel.try_into()?)
                            .await?
                            .into_iter()
                            .map(AltAccount::try_from)
                            .collect::<Result<Vec<_>, _>>()?,
                    )
                } else {
                    format.serialize(client.query_accounts(sel.try_into()?).await?)
                }
            }
            QueryTarget::Channel(sel) => format.serialize(client.query_channels(sel.try_into()?).await?),
            QueryTarget::SafesBalance { owner_address } => format.serialize(
                client
                    .query_safes_balance(owner_address.map(ChainAddress::from))
                    .await?,
            ),
            QueryTarget::ModuleAddress {
                nonce,
                owner,
                safe_address,
            } => format.serialize(
                client
                    .query_module_address_prediction(ModulePredictionInput {
                        nonce,
                        owner: owner.into(),
                        safe_address: safe_address.into(),
                    })
                    .await?,
            ),
            QueryTarget::TxCount { address } => {
                format.serialize(client.query_transaction_count(&address.into()).await?)
            }
            QueryTarget::SafeAllowance { address } => {
                format.serialize(client.query_safe_allowance(&address.into()).await?)
            }
            QueryTarget::CountAccounts(sel) => format.serialize(client.count_accounts(sel.try_into()?).await?),
            QueryTarget::CountChannels(sel) => format.serialize(client.count_channels(sel.try_into()?).await?),
            QueryTarget::Redemptions(sel) => format.serialize(client.query_redeemed_stats(sel.try_into()?).await?),
        }
    }
}
