use cynic::SubscriptionBuilder;
use futures::{Stream, TryStreamExt};

use super::{BlokliClient, GraphQlQueries};
use crate::api::{
    AccountSelector, BlokliSubscriptionClient, ChannelSelector, Result,
    internal::{
        AccountVariables, ChannelsVariables, SubscribeAccounts, SubscribeChannels, SubscribeGraph,
        SubscribeSafeDeployment, SubscribeTicketParams,
    },
    types::{Account, Channel, OpenedChannelsGraphEntry, Safe, TicketParameters},
};

impl GraphQlQueries {
    /// `SubscribeChannels` subscription GraphQL query.
    pub fn subscribe_channels(
        selector: ChannelSelector,
    ) -> cynic::StreamingOperation<SubscribeChannels, ChannelsVariables> {
        SubscribeChannels::build(ChannelsVariables::from(selector))
    }

    /// `SubscribeAccounts` subscription GraphQL query.
    pub fn subscribe_accounts(
        selector: AccountSelector,
    ) -> cynic::StreamingOperation<SubscribeAccounts, AccountVariables> {
        SubscribeAccounts::build(AccountVariables::from(selector))
    }

    /// `SubscribeGraph` subscription GraphQL query.
    pub fn subscribe_graph() -> cynic::StreamingOperation<SubscribeGraph, ()> {
        SubscribeGraph::build(())
    }

    /// `SubscribeTicketParams` subscription GraphQL query.
    pub fn subscribe_ticket_params() -> cynic::StreamingOperation<SubscribeTicketParams, ()> {
        SubscribeTicketParams::build(())
    }

    /// `SubscribeSafeDeployment` subscription GraphQL query.
    pub fn subscribe_safe_deployments() -> cynic::StreamingOperation<SubscribeSafeDeployment, ()> {
        SubscribeSafeDeployment::build(())
    }
}

impl BlokliSubscriptionClient for BlokliClient {
    #[tracing::instrument(level = "debug", skip(self), fields(?selector))]
    fn subscribe_channels(&self, selector: ChannelSelector) -> Result<impl Stream<Item = Result<Channel>> + Send> {
        Ok(self
            .build_subscription_stream(GraphQlQueries::subscribe_channels(selector))?
            .try_filter_map(|item| futures::future::ok(Some(item.channel_updated))))
    }

    #[tracing::instrument(level = "debug", skip(self), fields(?selector))]
    fn subscribe_accounts(&self, selector: AccountSelector) -> Result<impl Stream<Item = Result<Account>> + Send> {
        Ok(self
            .build_subscription_stream(GraphQlQueries::subscribe_accounts(selector))?
            .try_filter_map(|item| futures::future::ok(Some(item.account_updated))))
    }

    #[tracing::instrument(level = "debug", skip(self))]
    fn subscribe_graph(&self) -> Result<impl Stream<Item = Result<OpenedChannelsGraphEntry>> + Send> {
        Ok(self
            .build_subscription_stream(GraphQlQueries::subscribe_graph())?
            .try_filter_map(|item| futures::future::ok(Some(item.opened_channel_graph_updated))))
    }

    #[tracing::instrument(level = "debug", skip(self))]
    fn subscribe_ticket_params(&self) -> Result<impl Stream<Item = Result<TicketParameters>> + Send> {
        Ok(self
            .build_subscription_stream(GraphQlQueries::subscribe_ticket_params())?
            .try_filter_map(|item| futures::future::ok(Some(item.ticket_parameters_updated))))
    }

    #[tracing::instrument(level = "debug", skip(self))]
    fn subscribe_safe_deployments(&self) -> Result<impl Stream<Item = Result<Safe>> + Send> {
        Ok(self
            .build_subscription_stream(GraphQlQueries::subscribe_safe_deployments())?
            .try_filter_map(|item| futures::future::ok(Some(item.safe_deployed))))
    }
}
