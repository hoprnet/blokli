use super::BlokliClient;
use crate::api::{internal::*, types::*, *};
use crate::errors::ErrorKind;
use cynic::{GraphQlResponse, QueryBuilder};
use hex::ToHex;

fn response_to_data<Q>(response: GraphQlResponse<Q>) -> Result<Option<Q>> {
    match (response.data, response.errors) {
        (Some(data), None) => Ok(Some(data)),
        (Some(data), Some(errors)) => {
            tracing::error!(?errors, "operation succeeded but errors were encountered");
            Ok(Some(data))
        }
        (None, Some(errors)) => {
            if !errors.is_empty() {
                Err(ErrorKind::GraphQLError(errors.first().cloned().unwrap()).into())
            } else {
                Ok(None)
            }
        }
        (None, None) => Ok(None),
    }
}

#[async_trait::async_trait]
impl BlokliQueryClient for BlokliClient {
    async fn count_accounts(&self, selector: AccountSelector) -> Result<u32> {
        let resp = self
            .build_query(QueryAccountCount::build(AccountVariables::from(selector)))?
            .await?;

        response_to_data(resp)
            .and_then(|data| data.ok_or(ErrorKind::NoData.into()))
            .map(|data| data.account_count as u32)
    }

    async fn query_accounts(&self, selector: AccountSelector) -> Result<Vec<Account>> {
        let resp = self
            .build_query(QueryAccounts::build(AccountVariables::from(selector)))?
            .await?;

        response_to_data(resp).map(|data| data.map(|data| data.accounts).unwrap_or_default())
    }

    async fn query_native_balance(&self, address: &ChainAddress) -> Result<NativeBalance> {
        let resp = self
            .build_query(QueryAccountNativeBalance::build(BalanceVariables {
                address: address.encode_hex(),
            }))?
            .await?;

        response_to_data(resp).and_then(|data| data.and_then(|v| v.native_balance).ok_or(ErrorKind::NoData.into()))
    }

    async fn query_token_balance(&self, address: &ChainAddress) -> Result<HoprBalance> {
        let resp = self
            .build_query(QueryAccountHoprBalance::build(BalanceVariables {
                address: address.encode_hex(),
            }))?
            .await?;

        response_to_data(resp).and_then(|data| data.and_then(|v| v.hopr_balance).ok_or(ErrorKind::NoData.into()))
    }

    async fn query_channels(&self, selector: ChannelSelector) -> Result<Vec<Channel>> {
        let resp = self
            .build_query(QueryChannels::build(ChannelsVariables::from(selector)))?
            .await?;

        response_to_data(resp).map(|data| data.map(|data| data.channels).unwrap_or_default())
    }

    async fn query_chain_info(&self) -> Result<ChainInfo> {
        let resp = self.build_query(QueryChainInfo::build(()))?.await?;

        response_to_data(resp)
            .and_then(|data| data.ok_or(ErrorKind::NoData.into()))
            .map(|data| data.chain_info)
    }

    async fn query_version(&self) -> Result<String> {
        let resp = self.build_query(QueryVersion::build(()))?.await?;

        response_to_data(resp)
            .and_then(|data| data.ok_or(ErrorKind::NoData.into()))
            .map(|data| data.version)
    }

    async fn query_health(&self) -> Result<String> {
        let resp = self.build_query(QueryHealth::build(()))?.await?;

        response_to_data(resp)
            .and_then(|data| data.ok_or(ErrorKind::NoData.into()))
            .map(|data| data.health)
    }
}
