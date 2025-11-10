use cynic::QueryBuilder;
use hex::ToHex;
use hopr_primitive_types::{prelude::Address, traits::ToHex as PrimitiveToHex};

use super::{BlokliClient, response_to_data};
use crate::{
    api::{internal::*, types::*, *},
    errors::{BlokliClientError, ErrorKind},
};

#[async_trait::async_trait]
impl BlokliQueryClient for BlokliClient {
    #[tracing::instrument(level = "debug", skip(self), fields(?selector))]
    async fn count_accounts(&self, selector: Option<AccountSelector>) -> Result<u32> {
        let resp = self
            .build_query(QueryAccountCount::build(AccountVariables::from(selector)))?
            .await?;

        response_to_data(resp)?.account_count.into()
    }

    #[tracing::instrument(level = "debug", skip(self), fields(?selector))]
    async fn query_accounts(&self, selector: AccountSelector) -> Result<Vec<Account>> {
        let resp = self
            .build_query(QueryAccounts::build(AccountVariables::from(selector)))?
            .await?;

        response_to_data(resp)?.accounts.into()
    }

    #[tracing::instrument(level = "debug", skip(self), fields(address = Address::new(address).to_hex()))]
    async fn query_native_balance(&self, address: &ChainAddress) -> Result<NativeBalance> {
        let resp = self
            .build_query(QueryNativeBalance::build(BalanceVariables {
                address: address.encode_hex(),
            }))?
            .await?;

        response_to_data(resp)?.native_balance.into()
    }

    #[tracing::instrument(level = "debug", skip(self), fields(address = Address::new(address).to_hex()))]
    async fn query_token_balance(&self, address: &ChainAddress) -> Result<HoprBalance> {
        let resp = self
            .build_query(QueryHoprBalance::build(BalanceVariables {
                address: address.encode_hex(),
            }))?
            .await?;

        response_to_data(resp)?.hopr_balance.into()
    }

    #[tracing::instrument(level = "debug", skip(self), fields(address = Address::new(address).to_hex()))]
    async fn query_safe_allowance(&self, address: &ChainAddress) -> Result<SafeHoprAllowance> {
        let resp = self
            .build_query(QuerySafeAllowance::build(BalanceVariables {
                address: address.encode_hex(),
            }))?
            .await?;

        response_to_data(resp)?.safe_hopr_allowance.into()
    }

    #[tracing::instrument(level = "debug", skip(self), fields(?selector))]
    async fn count_channels(&self, selector: Option<ChannelSelector>) -> Result<u32> {
        let resp = self
            .build_query(QueryChannelCount::build(ChannelsVariables::from(selector)))?
            .await?;

        response_to_data(resp)?.channel_count.into()
    }

    #[tracing::instrument(level = "debug", skip(self), fields(?selector))]
    async fn query_channels(&self, selector: ChannelSelector) -> Result<Vec<Channel>> {
        let resp = self
            .build_query(QueryChannels::build(ChannelsVariables::from(selector)))?
            .await?;

        response_to_data(resp)?.channels.into()
    }

    #[tracing::instrument(level = "debug", skip(self))]
    async fn query_transaction_status(&self, tx_id: TxId) -> Result<Transaction> {
        let resp = self
            .build_query(QueryTransaction::build(TransactionsVariables { id: tx_id.into() }))?
            .await?;

        response_to_data(resp)?
            .transaction
            .ok_or::<BlokliClientError>(ErrorKind::NoData.into())?
            .into()
    }

    #[tracing::instrument(level = "debug", skip(self))]
    async fn query_chain_info(&self) -> Result<ChainInfo> {
        let resp = self.build_query(QueryChainInfo::build(()))?.await?;

        response_to_data(resp)?.chain_info.into()
    }

    #[tracing::instrument(level = "debug", skip(self))]
    async fn query_version(&self) -> Result<String> {
        let resp = self.build_query(QueryVersion::build(()))?.await?;

        response_to_data(resp).map(|data| data.version)
    }

    #[tracing::instrument(level = "debug", skip(self))]
    async fn query_health(&self) -> Result<String> {
        let resp = self.build_query(QueryHealth::build(()))?.await?;

        response_to_data(resp).map(|data| data.health)
    }
}
