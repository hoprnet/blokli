use cynic::SubscriptionBuilder;
use futures::{Stream, TryStreamExt};

use super::BlokliClient;
use crate::api::v1::TxId;
use crate::api::{internal::*, types::*, *};

impl BlokliSubscriptionClient for BlokliClient {
    #[tracing::instrument(level = "debug", skip(self), fields(?selector))]
    fn subscribe_channels(&self, selector: Option<ChannelSelector>) -> Result<impl Stream<Item = Result<Channel>>> {
        Ok(self
            .build_subscription_stream(SubscribeChannels::build(ChannelsVariables::from(selector)))?
            .try_filter_map(|item| futures::future::ok(Some(item.channel_updated))))
    }

    #[tracing::instrument(level = "debug", skip(self), fields(?selector))]
    fn subscribe_accounts(&self, selector: Option<AccountSelector>) -> Result<impl Stream<Item = Result<Account>>> {
        Ok(self
            .build_subscription_stream(SubscribeAccounts::build(AccountVariables::from(selector)))?
            .try_filter_map(|item| futures::future::ok(Some(item.account_updated))))
    }

    fn subscribe_transaction(&self, tx_id: TxId) -> Result<impl Stream<Item = Result<Transaction>>> {
        let mut was_confirmed = false;
        Ok(self
            .build_subscription_stream(SubscribeTransaction::build(TransactionsVariables { id: tx_id.into() }))?
            .try_filter_map(|item| futures::future::ok(Some(item.transaction_updated)))
            .try_take_while(move |item| {
                if was_confirmed {
                    futures::future::ok(false)
                } else {
                    was_confirmed = item.status != TransactionStatus::Confirmed;
                    futures::future::ok(true)
                }
            }))
    }
}
