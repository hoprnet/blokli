use cynic::{GraphQlResponse, QueryBuilder};
use futures::StreamExt;
use futures::stream::BoxStream;

use super::BlokliClient;
use crate::api::{types::*, *};
use crate::errors::{BlokliClientError, BlokliClientErrorKind};

fn response_to_data<Q>(response: GraphQlResponse<Q>) -> Result<Option<Q>, BlokliClientError> {
    match (response.data, response.errors) {
        (Some(data), None) => Ok(Some(data)),
        (Some(data), Some(errors)) => {
            tracing::error!(?errors, "operation succeeded but errors were encountered");
            Ok(Some(data))
        }
        (None, Some(errors)) => {
            if !errors.is_empty() {
                Err(BlokliClientErrorKind::GraphQLError(errors.first().cloned().unwrap()).into())
            } else {
                Ok(None)
            }
        }
        (None, None) => Ok(None),
    }
}

#[async_trait::async_trait]
impl BlokliQueryClient for BlokliClient {
    async fn count_accounts(&self, selector: AccountSelector) -> Result<u32, BlokliClientError> {
        let resp = self
            .build_query(QueryAccountCount::build(AccountVariables::from(selector)))?
            .await?;

        response_to_data(resp)
            .and_then(|data| data.ok_or(BlokliClientErrorKind::NoData.into()))
            .map(|data| data.account_count as u32)
    }

    async fn query_accounts<'a>(
        &'a self,
        selector: AccountSelector,
    ) -> Result<BoxStream<'a, Account>, BlokliClientError> {
        let resp = self
            .build_query(QueryAccounts::build(AccountVariables::from(selector)))?
            .await?;

        let accounts = response_to_data(resp).map(|data| data.map(|data| data.accounts).unwrap_or_default())?;

        Ok(futures::stream::iter(accounts).boxed())
    }

    async fn query_channels<'a>(
        &'a self,
        selector: ChannelSelector,
    ) -> Result<BoxStream<'a, Channel>, BlokliClientError> {
        let resp = self
            .build_query(QueryChannels::build(ChannelsVariables::from(selector)))?
            .await?;

        let channels = response_to_data(resp).map(|data| data.map(|data| data.channels).unwrap_or_default())?;

        Ok(futures::stream::iter(channels).boxed())
    }

    async fn query_chain_info(&self) -> Result<ChainInfo, BlokliClientError> {
        let resp = self.build_query(QueryChainInfo::build(()))?.await?;

        response_to_data(resp)
            .and_then(|data| data.ok_or(BlokliClientErrorKind::NoData.into()))
            .map(|data| data.chain_info)
    }

    async fn query_version(&self) -> Result<String, BlokliClientError> {
        let resp = self.build_query(QueryVersion::build(()))?.await?;

        response_to_data(resp)
            .and_then(|data| data.ok_or(BlokliClientErrorKind::NoData.into()))
            .map(|data| data.version)
    }

    async fn query_health(&self) -> Result<String, BlokliClientError> {
        let resp = self.build_query(QueryHealth::build(()))?.await?;

        response_to_data(resp)
            .and_then(|data| data.ok_or(BlokliClientErrorKind::NoData.into()))
            .map(|data| data.health)
    }
}
