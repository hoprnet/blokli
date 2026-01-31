use crate::{AccountArgs, AltAccount, ChannelArgs, Formats};
use blokli_client::{BlokliClient, api::BlokliSubscriptionClient};
use clap::{Subcommand, ValueEnum};
use futures::{StreamExt, TryStreamExt};

#[derive(Debug, Copy, Clone, ValueEnum)]
pub(crate) enum ChannelAllowedStates {
    /// Opened channels.
    Open,
    /// Pending to close channels.
    PendingToClose,
    /// Closed channels.
    Closed,
}

#[derive(Subcommand, Debug)]
pub(crate) enum SubscriptionTarget {
    /// Subscribe to Safe deployments.
    SafeDeployments,
    /// Subscribe to the graph updates.
    Graph,
    /// Subscribe to ticket parameter updates.
    TicketParams,
    /// Subscribe to account updates.
    Accounts(AccountArgs),
    /// Subscribe to channel updates.
    Channels(ChannelArgs),
}

impl SubscriptionTarget {
    pub(crate) fn execute(
        self,
        client: &BlokliClient,
        format: Formats,
    ) -> anyhow::Result<impl futures::Stream<Item = String> + Send> {
        match self {
            SubscriptionTarget::SafeDeployments => Ok(client
                .subscribe_safe_deployments()?
                .map_err(anyhow::Error::from)
                .filter_map(move |f| {
                    futures::future::ready(
                        f.and_then(|v| format.serialize(v))
                            .inspect_err(|e| eprintln!("failed to decode safe deployment event: {e}"))
                            .ok(),
                    )
                })
                .boxed()),
            SubscriptionTarget::Graph => Ok(client
                .subscribe_graph()?
                .map_err(anyhow::Error::from)
                .filter_map(move |f| {
                    futures::future::ready(
                        f.and_then(|v| format.serialize(v))
                            .inspect_err(|e| eprintln!("failed to decode graph event: {e}"))
                            .ok(),
                    )
                })
                .boxed()),
            SubscriptionTarget::TicketParams => Ok(client
                .subscribe_ticket_params()?
                .map_err(anyhow::Error::from)
                .filter_map(move |f| {
                    futures::future::ready(
                        f.and_then(|v| format.serialize(v))
                            .inspect_err(|e| eprintln!("failed to decode ticket params event: {e}"))
                            .ok(),
                    )
                })
                .boxed()),

            SubscriptionTarget::Accounts(sel) => {
                let serialize_alt_account = sel.show_peer_ids;
                Ok(client
                    .subscribe_accounts(sel.try_into()?)?
                    .map_err(anyhow::Error::from)
                    .filter_map(move |f| {
                        futures::future::ready(
                            f.and_then(|v| {
                                if serialize_alt_account {
                                    format.serialize(AltAccount::try_from(v)?)
                                } else {
                                    format.serialize(v)
                                }
                            })
                            .inspect_err(|e| eprintln!("failed to decode account event: {e}"))
                            .ok(),
                        )
                    })
                    .boxed())
            }
            SubscriptionTarget::Channels(sel) => Ok(client
                .subscribe_channels(sel.try_into()?)?
                .map_err(anyhow::Error::from)
                .filter_map(move |f| {
                    futures::future::ready(
                        f.and_then(|v| format.serialize(v))
                            .inspect_err(|e| eprintln!("failed to decode channel event: {e}"))
                            .ok(),
                    )
                })
                .boxed()),
        }
    }
}
