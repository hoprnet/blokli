use blokli_client::{BlokliClient, api::BlokliSubscriptionClient};
use clap::{Subcommand, ValueEnum};
use futures::StreamExt;

use crate::{AccountArgs, ChannelArgs, Formats};

#[derive(Debug, Copy, Clone, ValueEnum)]
pub enum ChannelAllowedStates {
    /// Opened channels.
    Open,
    /// Pending to close channels.
    PendingToClose,
    /// Closed channels.
    Closed,
}

#[derive(Subcommand, Debug)]
pub enum SubscriptionTarget {
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
    pub fn execute(
        self,
        client: &BlokliClient,
        format: Formats,
    ) -> anyhow::Result<impl futures::Stream<Item = String> + Send> {
        match self {
            SubscriptionTarget::SafeDeployments => Ok(client
                .subscribe_safe_deployments()?
                .filter_map(move |f| futures::future::ready(f.ok().map(|v| format.serialize(v))))
                .boxed()),
            SubscriptionTarget::Graph => Ok(client
                .subscribe_graph()?
                .filter_map(move |f| futures::future::ready(f.ok().map(|v| format.serialize(v))))
                .boxed()),
            SubscriptionTarget::TicketParams => Ok(client
                .subscribe_ticket_params()?
                .filter_map(move |f| futures::future::ready(f.ok().map(|v| format.serialize(v))))
                .boxed()),

            SubscriptionTarget::Accounts(sel) => Ok(client
                .subscribe_accounts(Some(sel.try_into()?))?
                .filter_map(move |f| futures::future::ready(f.ok().map(|v| format.serialize(v))))
                .boxed()),
            SubscriptionTarget::Channels(sel) => Ok(client
                .subscribe_channels(Some(sel.try_into()?))?
                .filter_map(move |f| futures::future::ready(f.ok().map(|v| format.serialize(v))))
                .boxed()),
        }
    }
}
