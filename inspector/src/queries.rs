use std::collections::{HashMap, HashSet};

use blokli_client::{
    BlokliClient,
    api::{
        AccountSelector, BlokliQueryClient, ChainAddress, ChannelFilter, ChannelSelector, ModulePredictionInput,
        SafeSelector,
        types::{Account, ChainInfo, Channel, ChannelStatus, ChannelsList, TokenValueString},
    },
};
use clap::Subcommand;
use futures::{StreamExt, TryStreamExt, stream};
use hopr_types::primitive::prelude::{Address, HoprBalance};

use crate::{AccountArgs, AltAccount, ChannelArgs, Formats, RedemptionsArgs, node_account_selector};

const ACCOUNT_QUERY_CONCURRENCY: usize = 8;

#[derive(Debug, serde::Serialize)]
struct NodeOverview {
    source: AltAccount,
    ticket_parameters: OverviewTicketParameters,
    summary: NodeOverviewSummary,
    channels: Vec<NodeOverviewChannel>,
    incoming_channels: Vec<NodeOverviewIncomingChannel>,
}

#[derive(Debug, serde::Serialize)]
struct OverviewTicketParameters {
    block_number: i32,
    ticket_price: TokenValueString,
    min_ticket_winning_probability: f64,
}

#[derive(Debug, serde::Serialize)]
struct NodeOverviewSummary {
    channel_count: usize,
    open_count: usize,
    pending_to_close_count: usize,
    closed_count: usize,
    total_balance: String,
}

#[derive(Debug, serde::Serialize)]
struct NodeOverviewChannel {
    channel: Channel,
    destination: AltAccount,
}

#[derive(Debug, serde::Serialize)]
struct NodeOverviewIncomingChannel {
    channel: Channel,
    source: AltAccount,
}

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
        /// Safe chain key address.
        #[arg(long, value_parser = clap::value_parser!(Address), group = "selector")]
        chain_key: Option<Address>,
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
    /// Gets a node and its channels, enriched with counterparty and ticket details.
    NodeOverview {
        /// Node chain key, hex packet key, or Peer ID.
        node: String,
    },
    /// Gets channel count and total wxHOPR balance matching optional filters.
    ChannelStats(ChannelArgs),
    /// Gets channels with their aggregated wxHOPR balance. Use --safe-address to scope to a safe.
    Channel(ChannelArgs),
    /// Gets total wxHOPR balance held across indexed safe contracts.
    SafesBalance {
        /// Restrict to safes whose current owner set contains the given address.
        #[arg(long, value_parser = clap::value_parser!(Address))]
        owner: Option<Address>,
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
                chain_key,
                registered_node,
            } => format.serialize(
                client
                    .query_safe(match (address, owner, chain_key, registered_node) {
                        (Some(address), None, None, None) => SafeSelector::SafeAddress(address.into()),
                        (None, Some(owner), None, None) => SafeSelector::Owner(owner.into()),
                        (None, None, Some(chain_key), None) => SafeSelector::ChainKey(chain_key.into()),
                        (None, None, None, Some(registered_node)) => {
                            SafeSelector::RegisteredNode(registered_node.into())
                        }
                        _ => {
                            return Err(anyhow::anyhow!(
                                "Exactly one of --address, --owner, --chain-key or --registered-node must be \
                                 specified."
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
            QueryTarget::NodeOverview { node } => query_node_overview(client, node_account_selector(&node)?)
                .await
                .and_then(|overview| format.serialize(overview)),
            QueryTarget::Channel(sel) => format.serialize(client.query_channels(sel.try_into()?).await?),
            QueryTarget::SafesBalance { owner } => {
                format.serialize(client.query_safes_balance(owner.map(ChainAddress::from)).await?)
            }
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
            QueryTarget::ChannelStats(sel) => format.serialize(client.query_channel_stats(sel.try_into()?).await?),
            QueryTarget::Redemptions(sel) => format.serialize(client.query_redeemed_stats(sel.try_into()?).await?),
        }
    }
}

async fn query_node_overview(client: &BlokliClient, selector: AccountSelector) -> anyhow::Result<NodeOverview> {
    let source = exactly_one_account(client.query_accounts(selector).await?, "source")?;
    let source_key_id = u32::try_from(source.keyid)?;
    let (channels, incoming_channels, chain_info) = tokio::try_join!(
        client.query_channels(ChannelSelector {
            filter: Some(ChannelFilter::SourceKeyId(source_key_id)),
            ..Default::default()
        }),
        client.query_channels(ChannelSelector {
            filter: Some(ChannelFilter::DestinationKeyId(source_key_id)),
            ..Default::default()
        }),
        client.query_chain_info(),
    )?;
    let account_ids = channels
        .channels
        .iter()
        .map(|channel| channel.destination)
        .chain(incoming_channels.channels.iter().map(|channel| channel.source))
        .collect::<HashSet<_>>();
    let accounts = stream::iter(account_ids)
        .map(|account_id| async move {
            let key_id = u32::try_from(account_id)?;
            let account = exactly_one_account(
                client.query_accounts(AccountSelector::KeyId(key_id)).await?,
                "counterparty",
            )?;
            Ok::<_, anyhow::Error>((account_id, AltAccount::try_from(account)?))
        })
        .buffer_unordered(ACCOUNT_QUERY_CONCURRENCY)
        .try_collect::<HashMap<_, _>>()
        .await?;

    build_node_overview(source, channels, incoming_channels, chain_info, accounts)
}

fn exactly_one_account(accounts: Vec<Account>, role: &str) -> anyhow::Result<Account> {
    let [account]: [Account; 1] = accounts.try_into().map_err(|accounts: Vec<Account>| {
        anyhow::anyhow!("Expected exactly one {role} account, got {}", accounts.len())
    })?;
    Ok(account)
}

fn build_node_overview(
    source: Account,
    channels: ChannelsList,
    incoming_channels: ChannelsList,
    chain_info: ChainInfo,
    accounts: HashMap<i32, AltAccount>,
) -> anyhow::Result<NodeOverview> {
    let mut total_balance = HoprBalance::zero();
    let mut open_count = 0;
    let mut pending_to_close_count = 0;
    let mut closed_count = 0;
    let overview_channels = channels
        .channels
        .into_iter()
        .map(|channel| {
            total_balance += channel.balance.0.parse::<HoprBalance>()?;
            match channel.status {
                ChannelStatus::Open => open_count += 1,
                ChannelStatus::PendingToClose => pending_to_close_count += 1,
                ChannelStatus::Closed => closed_count += 1,
            }
            let destination = accounts
                .get(&channel.destination)
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("Missing destination account for key ID {}", channel.destination))?;
            Ok(NodeOverviewChannel { channel, destination })
        })
        .collect::<anyhow::Result<Vec<_>>>()?;
    let overview_incoming_channels = incoming_channels
        .channels
        .into_iter()
        .map(|channel| {
            let source = accounts
                .get(&channel.source)
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("Missing source account for key ID {}", channel.source))?;
            Ok(NodeOverviewIncomingChannel { channel, source })
        })
        .collect::<anyhow::Result<Vec<_>>>()?;

    Ok(NodeOverview {
        source: AltAccount::try_from(source)?,
        ticket_parameters: OverviewTicketParameters {
            block_number: chain_info.block_number,
            ticket_price: chain_info.ticket_price,
            min_ticket_winning_probability: chain_info.min_ticket_winning_probability,
        },
        summary: NodeOverviewSummary {
            channel_count: overview_channels.len(),
            open_count,
            pending_to_close_count,
            closed_count,
            total_balance: total_balance.to_string(),
        },
        channels: overview_channels,
        incoming_channels: overview_incoming_channels,
    })
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use blokli_client::api::types::{
        Account, ChainInfo, Channel, ChannelStatus, ChannelsList, ContractAddressMap, TokenValueString, Uint64,
    };

    use super::{AltAccount, build_node_overview};
    use crate::Formats;

    const PACKET_KEY: &str = "30dc46df1f429b9c0d1d6d81198420f3af92348e7fe97b003717108b22f8d985";

    fn account(keyid: i32) -> Account {
        Account {
            chain_key: format!("0x{keyid:040x}"),
            keyid,
            multi_addresses: Vec::new(),
            packet_key: PACKET_KEY.to_string(),
            safe_address: None,
        }
    }

    fn channel(destination: i32, status: ChannelStatus, balance: &str) -> Channel {
        Channel {
            balance: TokenValueString(balance.to_string()),
            closure_time: None,
            concrete_channel_id: format!("{destination:064x}"),
            destination,
            epoch: 1,
            source: 7,
            status,
            ticket_index: Uint64("0".to_string()),
        }
    }

    fn incoming_channel(source: i32, status: ChannelStatus, balance: &str) -> Channel {
        Channel {
            balance: TokenValueString(balance.to_string()),
            closure_time: None,
            concrete_channel_id: format!("{source:064x}"),
            destination: 7,
            epoch: 1,
            source,
            status,
            ticket_index: Uint64("0".to_string()),
        }
    }

    fn chain_info() -> ChainInfo {
        ChainInfo {
            block_number: 123,
            chain_id: 100,
            network: "rotsee".to_string(),
            ticket_price: TokenValueString("0.0000000000000001 wxHOPR".to_string()),
            key_binding_fee: TokenValueString("0.01 wxHOPR".to_string()),
            gas_price: None,
            max_fee_per_gas: None,
            max_priority_fee_per_gas: None,
            min_ticket_winning_probability: 0.000125,
            channel_dst: None,
            contract_addresses: ContractAddressMap("{}".to_string()),
            ledger_dst: None,
            safe_registry_dst: None,
            channel_closure_grace_period: Uint64("300".to_string()),
            expected_block_time: Uint64("5".to_string()),
            finality: Uint64("3".to_string()),
        }
    }

    #[test]
    fn overview_enriches_channels_and_aggregates_summary() -> anyhow::Result<()> {
        let channels = ChannelsList {
            __typename: "ChannelsList".to_string(),
            channels: vec![
                channel(2, ChannelStatus::Open, "1 wxHOPR"),
                channel(3, ChannelStatus::PendingToClose, "0.5 wxHOPR"),
                channel(4, ChannelStatus::Closed, "0 wxHOPR"),
            ],
        };
        let incoming_channels = ChannelsList {
            __typename: "ChannelsList".to_string(),
            channels: vec![incoming_channel(5, ChannelStatus::Open, "2 wxHOPR")],
        };
        let accounts = [2, 3, 4, 5]
            .into_iter()
            .map(|keyid| Ok((keyid, AltAccount::try_from(account(keyid))?)))
            .collect::<anyhow::Result<HashMap<_, _>>>()?;

        let overview = build_node_overview(account(7), channels, incoming_channels, chain_info(), accounts)?;

        assert_eq!(overview.ticket_parameters.block_number, 123);
        assert_eq!(overview.ticket_parameters.ticket_price.0, "0.0000000000000001 wxHOPR");
        assert_eq!(overview.ticket_parameters.min_ticket_winning_probability, 0.000125);
        assert_eq!(overview.summary.channel_count, 3);
        assert_eq!(overview.summary.open_count, 1);
        assert_eq!(overview.summary.pending_to_close_count, 1);
        assert_eq!(overview.summary.closed_count, 1);
        assert_eq!(overview.summary.total_balance, "1.5 wxHOPR");
        assert_eq!(
            overview
                .channels
                .iter()
                .map(|entry| entry.destination.keyid)
                .collect::<Vec<_>>(),
            vec![2, 3, 4]
        );
        assert_eq!(
            overview
                .incoming_channels
                .iter()
                .map(|entry| entry.source.keyid)
                .collect::<Vec<_>>(),
            vec![5]
        );
        insta::assert_snapshot!(Formats::Table.serialize(&overview)?);
        Ok(())
    }

    #[test]
    fn overview_rejects_missing_destination() -> anyhow::Result<()> {
        let channels = ChannelsList {
            __typename: "ChannelsList".to_string(),
            channels: vec![channel(2, ChannelStatus::Open, "1 wxHOPR")],
        };
        let incoming_channels = ChannelsList {
            __typename: "ChannelsList".to_string(),
            channels: Vec::new(),
        };

        let result = build_node_overview(account(7), channels, incoming_channels, chain_info(), HashMap::new());

        assert!(result.is_err());
        Ok(())
    }

    #[test]
    fn overview_rejects_missing_incoming_source() -> anyhow::Result<()> {
        let channels = ChannelsList {
            __typename: "ChannelsList".to_string(),
            channels: Vec::new(),
        };
        let incoming_channels = ChannelsList {
            __typename: "ChannelsList".to_string(),
            channels: vec![incoming_channel(5, ChannelStatus::Open, "2 wxHOPR")],
        };

        let result = build_node_overview(account(7), channels, incoming_channels, chain_info(), HashMap::new());

        assert!(result.is_err());
        Ok(())
    }
}
