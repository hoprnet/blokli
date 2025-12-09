use cynic::QueryBuilder;
use hex::ToHex;

use super::{BlokliClient, GraphQlQueries, response_to_data};
use crate::{
    api::{internal::*, types::*, *},
    errors::{BlokliClientError, ErrorKind},
};

impl GraphQlQueries {
    /// `AccountCount` GraphQL query.
    pub fn count_accounts(selector: Option<AccountSelector>) -> cynic::Operation<QueryAccountCount, AccountVariables> {
        QueryAccountCount::build(AccountVariables::from(selector))
    }

    /// `Accounts` GraphQL query.
    pub fn query_accounts(selector: AccountSelector) -> cynic::Operation<QueryAccounts, AccountVariables> {
        QueryAccounts::build(AccountVariables::from(selector))
    }

    /// `NativeBalance` GraphQL query.
    pub fn query_native_balance(address: &ChainAddress) -> cynic::Operation<QueryNativeBalance, BalanceVariables> {
        QueryNativeBalance::build(BalanceVariables {
            address: address.encode_hex(),
        })
    }

    /// `TokenBalance` GraphQL query.
    pub fn query_token_balance(address: &ChainAddress) -> cynic::Operation<QueryHoprBalance, BalanceVariables> {
        QueryHoprBalance::build(BalanceVariables {
            address: address.encode_hex(),
        })
    }

    /// `TransactionCount` GraphQL query.
    pub fn query_transaction_count(address: &ChainAddress) -> cynic::Operation<QueryTxCount, TxCountVariables> {
        QueryTxCount::build(TxCountVariables {
            address: address.encode_hex(),
        })
    }

    /// `SafeAllowance` GraphQL query.
    pub fn query_safe_allowance(address: &ChainAddress) -> cynic::Operation<QuerySafeAllowance, BalanceVariables> {
        QuerySafeAllowance::build(BalanceVariables {
            address: address.encode_hex(),
        })
    }

    /// `Safe` GraphQL query.
    pub fn query_safe_by_address(address: &ChainAddress) -> cynic::Operation<QuerySafeByAddress, SafeVariables> {
        QuerySafeByAddress::build(SafeVariables {
            address: address.encode_hex(),
        })
    }

    /// `Safe` GraphQL query.
    pub fn query_safe_by_chain_key(address: &ChainAddress) -> cynic::Operation<QuerySafeByChainKey, SafeVariables> {
        QuerySafeByChainKey::build(SafeVariables {
            address: address.encode_hex(),
        })
    }

    /// `ChannelCount` GraphQL query.
    pub fn query_channel_count(
        selector: Option<ChannelSelector>,
    ) -> cynic::Operation<QueryChannelCount, ChannelsVariables> {
        QueryChannelCount::build(ChannelsVariables::from(selector))
    }

    /// `Channels` GraphQL query.
    pub fn query_channels(selector: ChannelSelector) -> cynic::Operation<QueryChannels, ChannelsVariables> {
        QueryChannels::build(ChannelsVariables::from(selector))
    }

    /// `Transaction` GraphQL query.
    pub fn query_transaction(id: TxId) -> cynic::Operation<QueryTransaction, TransactionsVariables> {
        QueryTransaction::build(TransactionsVariables { id: id.into() })
    }

    /// `ChainInfo` GraphQL query.
    pub fn query_chain_info() -> cynic::Operation<QueryChainInfo, ()> {
        QueryChainInfo::build(())
    }

    /// `Version` GraphQL query.
    pub fn query_version() -> cynic::Operation<QueryVersion, ()> {
        QueryVersion::build(())
    }

    /// `Health` GraphQL query.
    pub fn query_health() -> cynic::Operation<QueryHealth, ()> {
        QueryHealth::build(())
    }
}

#[async_trait::async_trait]
impl BlokliQueryClient for BlokliClient {
    #[tracing::instrument(level = "debug", skip(self), fields(?selector))]
    async fn count_accounts(&self, selector: Option<AccountSelector>) -> Result<u32> {
        let resp = self.build_query(GraphQlQueries::count_accounts(selector))?.await?;

        response_to_data(resp)?.account_count.into()
    }

    #[tracing::instrument(level = "debug", skip(self), fields(?selector))]
    async fn query_accounts(&self, selector: AccountSelector) -> Result<Vec<Account>> {
        let resp = self.build_query(GraphQlQueries::query_accounts(selector))?.await?;

        response_to_data(resp)?.accounts.into()
    }

    #[tracing::instrument(level = "debug", skip(self), fields(address = hex::encode(address)))]
    async fn query_native_balance(&self, address: &ChainAddress) -> Result<NativeBalance> {
        let resp = self.build_query(GraphQlQueries::query_native_balance(address))?.await?;

        response_to_data(resp)?.native_balance.into()
    }

    #[tracing::instrument(level = "debug", skip(self), fields(address = hex::encode(address)))]
    async fn query_token_balance(&self, address: &ChainAddress) -> Result<HoprBalance> {
        let resp = self.build_query(GraphQlQueries::query_token_balance(address))?.await?;

        response_to_data(resp)?.hopr_balance.into()
    }

    #[tracing::instrument(level = "debug", skip(self), fields(address = hex::encode(address)))]
    async fn query_transaction_count(&self, address: &ChainAddress) -> Result<u64> {
        let resp = self
            .build_query(GraphQlQueries::query_transaction_count(address))?
            .await?;

        response_to_data(resp)?.safe_transaction_count.into()
    }

    #[tracing::instrument(level = "debug", skip(self), fields(address = hex::encode(address)))]
    async fn query_safe_allowance(&self, address: &ChainAddress) -> Result<SafeHoprAllowance> {
        let resp = self.build_query(GraphQlQueries::query_safe_allowance(address))?.await?;

        response_to_data(resp)?.safe_hopr_allowance.into()
    }

    #[tracing::instrument(level = "debug", skip(self), fields(?selector))]
    async fn query_safe(&self, selector: SafeSelector) -> Result<Option<Safe>> {
        match selector {
            SafeSelector::SafeAddress(safe_addr) => {
                let res = self
                    .build_query(GraphQlQueries::query_safe_by_address(&safe_addr))?
                    .await?;

                match response_to_data(res)?.safe {
                    Some(result) => result.into(),
                    None => Ok(None),
                }
            }
            SafeSelector::ChainKey(chain_addr) => {
                let res = self
                    .build_query(GraphQlQueries::query_safe_by_chain_key(&chain_addr))?
                    .await?;

                match response_to_data(res)?.safe_by_chain_key {
                    Some(result) => result.into(),
                    None => Ok(None),
                }
            }
        }
    }

    #[tracing::instrument(level = "debug", skip(self), fields(?selector))]
    async fn count_channels(&self, selector: Option<ChannelSelector>) -> Result<u32> {
        let resp = self.build_query(GraphQlQueries::query_channel_count(selector))?.await?;

        response_to_data(resp)?.channel_count.into()
    }

    #[tracing::instrument(level = "debug", skip(self), fields(?selector))]
    async fn query_channels(&self, selector: ChannelSelector) -> Result<Vec<Channel>> {
        if selector.filter.is_none() {
            return Err(ErrorKind::InvalidInput("filter must be specified on channel query").into());
        }

        let resp = self.build_query(GraphQlQueries::query_channels(selector))?.await?;
        response_to_data(resp)?.channels.into()
    }

    #[tracing::instrument(level = "debug", skip(self))]
    async fn query_transaction_status(&self, tx_id: TxId) -> Result<Transaction> {
        let resp = self.build_query(GraphQlQueries::query_transaction(tx_id))?.await?;

        response_to_data(resp)?
            .transaction
            .ok_or::<BlokliClientError>(ErrorKind::NoData.into())?
            .into()
    }

    #[tracing::instrument(level = "debug", skip(self))]
    async fn query_chain_info(&self) -> Result<ChainInfo> {
        let resp = self.build_query(GraphQlQueries::query_chain_info())?.await?;

        response_to_data(resp)?.chain_info.into()
    }

    #[tracing::instrument(level = "debug", skip(self))]
    async fn query_version(&self) -> Result<String> {
        let resp = self.build_query(GraphQlQueries::query_version())?.await?;

        response_to_data(resp).map(|data| data.version)
    }

    #[tracing::instrument(level = "debug", skip(self))]
    async fn query_health(&self) -> Result<String> {
        let resp = self.build_query(GraphQlQueries::query_health())?.await?;

        response_to_data(resp).map(|data| data.health)
    }
}
