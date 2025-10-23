use cynic::SubscriptionBuilder;
use futures::{Stream, TryStreamExt};
use hex::ToHex;

use super::BlokliClient;
use crate::api::{internal::*, types::*, *};

impl BlokliSubscriptionClient for BlokliClient {
    fn subscribe_native_balance(&self, address: &ChainAddress) -> Result<impl Stream<Item = Result<NativeBalance>>> {
        Ok(self
            .build_subscription_stream(SubscribeAccountNativeBalance::build(BalanceVariables {
                address: address.encode_hex(),
            }))?
            .try_filter_map(|item| futures::future::ok(item.native_balance_updated)))
    }

    fn subscribe_token_balance(&self, address: &ChainAddress) -> Result<impl Stream<Item = Result<HoprBalance>>> {
        Ok(self
            .build_subscription_stream(SubscribeAccountHoprBalance::build(BalanceVariables {
                address: address.encode_hex(),
            }))?
            .try_filter_map(|item| futures::future::ok(item.hopr_balance_updated)))
    }

    fn subscribe_channels(&self, selector: Option<ChannelSelector>) -> Result<impl Stream<Item = Result<Channel>>> {
        Ok(self
            .build_subscription_stream(SubscribeChannels::build(ChannelsVariables::from(selector)))?
            .try_filter_map(|item| futures::future::ok(Some(item.channel_updated))))
    }

    fn subscribe_accounts(&self, selector: Option<AccountSelector>) -> Result<impl Stream<Item = Result<Account>>> {
        Ok(self
            .build_subscription_stream(SubscribeAccounts::build(AccountVariables::from(selector)))?
            .try_filter_map(|item| futures::future::ok(Some(item.account_updated))))
    }
}
