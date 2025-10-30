use cynic::SubscriptionBuilder;
use futures::{Stream, TryStreamExt};

use super::BlokliClient;
use crate::api::{internal::*, types::*, *};

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
}
