use cynic::SubscriptionBuilder;
use futures::{Stream, TryStreamExt};

use super::BlokliClient;
use crate::api::{
    AccountSelector, BlokliSubscriptionClient, ChannelSelector, Result,
    internal::{
        AccountVariables, ChannelsVariables, SubscribeAccounts, SubscribeChannels, SubscribeGraph,
        SubscribeTicketParams,
    },
    types::{Account, Channel, OpenedChannelsGraphEntry, TicketParameters},
};

impl BlokliSubscriptionClient for BlokliClient {
    #[tracing::instrument(level = "debug", skip(self), fields(?selector))]
    fn subscribe_channels(
        &self,
        selector: Option<ChannelSelector>,
    ) -> Result<impl Stream<Item = Result<Channel>> + Send> {
        Ok(self
            .build_subscription_stream(SubscribeChannels::build(ChannelsVariables::from(selector)))?
            .try_filter_map(|item| futures::future::ok(Some(item.channel_updated))))
    }

    #[tracing::instrument(level = "debug", skip(self), fields(?selector))]
    fn subscribe_accounts(
        &self,
        selector: Option<AccountSelector>,
    ) -> Result<impl Stream<Item = Result<Account>> + Send> {
        Ok(self
            .build_subscription_stream(SubscribeAccounts::build(AccountVariables::from(selector)))?
            .try_filter_map(|item| futures::future::ok(Some(item.account_updated))))
    }

    #[tracing::instrument(level = "debug", skip(self))]
    fn subscribe_graph(&self) -> Result<impl Stream<Item = Result<OpenedChannelsGraphEntry>> + Send> {
        Ok(self
            .build_subscription_stream(SubscribeGraph::build(()))?
            .try_filter_map(|item| futures::future::ok(Some(item.opened_channel_graph_updated))))
    }

    #[tracing::instrument(level = "debug", skip(self))]
    fn subscribe_ticket_params(&self) -> Result<impl Stream<Item = Result<TicketParameters>> + Send> {
        Ok(self
            .build_subscription_stream(SubscribeTicketParams::build(()))?
            .try_filter_map(|item| futures::future::ok(Some(item.ticket_parameters_updated))))
    }
}
