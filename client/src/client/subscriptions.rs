use cynic::SubscriptionBuilder;
use futures::{StreamExt, TryStreamExt};

use super::BlokliClient;
use crate::api::{types::*, *};
use crate::errors::BlokliClientError;

#[async_trait::async_trait]
impl BlokliSubscriptionClient for BlokliClient {
    async fn subscribe_native_balance<'a>(
        &'a self,
        address: String,
    ) -> Result<EventStream<'a, NativeBalance>, BlokliClientError> {
        Ok(self
            .build_subscription_stream(SubscribeAccountNativeBalance::build(BalanceVariables { address }))?
            .try_filter_map(|item| futures::future::ok(item.native_balance_updated))
            .boxed())
    }

    async fn subscribe_token_balance<'a>(
        &'a self,
        address: String,
    ) -> Result<EventStream<'a, HoprBalance>, BlokliClientError> {
        Ok(self
            .build_subscription_stream(SubscribeAccountHoprBalance::build(BalanceVariables { address }))?
            .try_filter_map(|item| futures::future::ok(item.hopr_balance_updated))
            .boxed())
    }

    async fn subscribe_channels<'a>(
        &'a self,
        selector: Option<ChannelSelector>,
    ) -> Result<EventStream<'a, Channel>, BlokliClientError> {
        Ok(self
            .build_subscription_stream(SubscribeChannels::build(ChannelsVariables::from(selector)))?
            .try_filter_map(|item| futures::future::ok(Some(item.channel_updated)))
            .boxed())
    }

    async fn subscribe_accounts<'a>(
        &'a self,
        selector: Option<AccountSelector>,
    ) -> Result<EventStream<'a, Account>, BlokliClientError> {
        Ok(self
            .build_subscription_stream(SubscribeAccounts::build(AccountVariables::from(selector)))?
            .try_filter_map(|item| futures::future::ok(Some(item.account_updated)))
            .boxed())
    }
}
