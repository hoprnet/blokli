use std::{any::Any, fmt::Debug, marker::PhantomData};

use cynic::GraphQlResponse;

use super::{BlokliClient, GraphQlQueries, response_to_data};
use crate::{
    api::{
        AccountSelector, ChainAddress, ChannelSelector, ModulePredictionInput, RedeemedStatsSelector, SafeSelector,
        TxId,
        internal::SafeSelectorInput,
        types::{
            Account, ChainInfo, ChannelStats, ChannelsList, Compatibility, HoprBalance, NativeBalance, RedeemedStats,
            Safe, SafeHoprAllowance, SafesBalance, Transaction,
        },
    },
    errors::{BlokliClientError, ErrorKind},
};

type ClientResult<T> = std::result::Result<T, BlokliClientError>;
type BatchParser = Box<dyn FnOnce(serde_json::Value) -> ClientResult<Box<dyn Any + Send>> + Send>;

struct QueuedQuery {
    operation: serde_json::Value,
    parser: BatchParser,
}

/// Typed handle for a query queued in a [`BlokliQueryBatch`].
///
/// A handle is returned when a query is added to a batch. After executing the batch, pass the handle to
/// [`BlokliQueryBatchResult::take`] to retrieve that query's typed result.
#[derive(Clone, Copy, Debug)]
pub struct QueryHandle<T> {
    index: usize,
    phantom: PhantomData<fn() -> T>,
}

/// Builder for executing multiple read-only Blokli GraphQL queries in a single HTTP request.
///
/// The batch API is query-only. It does not support subscriptions or transaction mutations. Each queued query returns
/// a [`QueryHandle`] that preserves the response type for that query.
///
/// When automatic compatibility checking is enabled, executing a batch performs the same compatibility preflight as a
/// single query. If the compatibility result is not cached yet, that preflight is a separate HTTP request before the
/// batched query request.
///
/// ```
/// # async fn example(client: blokli_client::BlokliClient) -> Result<(), blokli_client::errors::BlokliClientError> {
/// use blokli_client::api::AccountSelector;
///
/// let mut batch = client.query_batch();
/// let first = batch.query_accounts(AccountSelector::Address([0x11; 20]))?;
/// let second = batch.query_accounts(AccountSelector::Address([0x22; 20]))?;
///
/// let mut results = batch.execute().await?;
/// let first_accounts = results.take(first)?;
/// let second_accounts = results.take(second)?;
/// # let _ = (first_accounts, second_accounts);
/// # Ok(())
/// # }
/// ```
pub struct BlokliQueryBatch {
    client: BlokliClient,
    queries: Vec<QueuedQuery>,
}

impl BlokliQueryBatch {
    pub(crate) fn new(client: BlokliClient) -> Self {
        Self {
            client,
            queries: Vec::new(),
        }
    }

    fn push<Q, V, T, F>(&mut self, op: cynic::Operation<Q, V>, extract: F) -> ClientResult<QueryHandle<T>>
    where
        Q: cynic::QueryFragment + cynic::serde::de::DeserializeOwned + Debug + 'static,
        V: cynic::QueryVariables + cynic::serde::Serialize,
        T: Send + 'static,
        F: FnOnce(Q) -> ClientResult<T> + Send + 'static,
    {
        let index = self.queries.len();
        let operation = serde_json::to_value(&op).map_err(ErrorKind::from)?;
        let parser = Box::new(move |value| {
            let response = serde_json::from_value::<GraphQlResponse<Q>>(value).map_err(BlokliClientError::from)?;
            let data = response_to_data(response)?;
            let value = extract(data)?;
            Ok(Box::new(value) as Box<dyn Any + Send>)
        });

        self.queries.push(QueuedQuery { operation, parser });
        Ok(QueryHandle {
            index,
            phantom: PhantomData,
        })
    }

    /// Counts the number of accounts optionally matching the given selector.
    pub fn count_accounts(&mut self, selector: AccountSelector) -> ClientResult<QueryHandle<u32>> {
        self.push(GraphQlQueries::count_accounts(selector), |data| {
            data.account_count.into()
        })
    }

    /// Queries the accounts matching the given selector.
    pub fn query_accounts(&mut self, selector: AccountSelector) -> ClientResult<QueryHandle<Vec<Account>>> {
        if matches!(selector, AccountSelector::Any) {
            return Err(ErrorKind::InvalidInput("filter must be specified on account query").into());
        }

        self.push(GraphQlQueries::query_accounts(selector), |data| data.accounts.into())
    }

    /// Queries the native balance of the given account.
    pub fn query_native_balance(&mut self, address: &ChainAddress) -> ClientResult<QueryHandle<NativeBalance>> {
        self.push(GraphQlQueries::query_native_balance(address), |data| {
            data.native_balance.into()
        })
    }

    /// Queries the token balance of the given account.
    pub fn query_token_balance(&mut self, address: &ChainAddress) -> ClientResult<QueryHandle<HoprBalance>> {
        self.push(GraphQlQueries::query_token_balance(address), |data| {
            data.hopr_balance.into()
        })
    }

    /// Queries the number of transactions sent from the given account.
    pub fn query_transaction_count(&mut self, address: &ChainAddress) -> ClientResult<QueryHandle<u64>> {
        self.push(GraphQlQueries::query_transaction_count(address), |data| {
            data.transaction_count.into()
        })
    }

    /// Queries the safe allowance of the given account.
    pub fn query_safe_allowance(&mut self, address: &ChainAddress) -> ClientResult<QueryHandle<SafeHoprAllowance>> {
        self.push(GraphQlQueries::query_safe_allowance(address), |data| {
            data.safe_hopr_allowance.into()
        })
    }

    /// Queries redeemed ticket stats filtered by safe, node, or both.
    pub fn query_redeemed_stats(
        &mut self,
        selector: RedeemedStatsSelector,
    ) -> ClientResult<QueryHandle<RedeemedStats>> {
        self.push(GraphQlQueries::query_redeemed_stats(selector), |data| {
            data.ticket_redemption_stats.into()
        })
    }

    /// Queries deployed Safes matching the given selector.
    pub fn query_safe(&mut self, selector: SafeSelector) -> ClientResult<QueryHandle<Vec<Safe>>> {
        let (gql_selector, addr) = match selector {
            SafeSelector::SafeAddress(addr) => (SafeSelectorInput::Address, addr),
            SafeSelector::Owner(addr) => (SafeSelectorInput::Owner, addr),
            SafeSelector::ChainKey(addr) => (SafeSelectorInput::ChainKey, addr),
            SafeSelector::RegisteredNode(addr) => (SafeSelectorInput::RegisteredNode, addr),
        };

        self.push(GraphQlQueries::query_safe_by(gql_selector, &addr), |data| {
            match data.safe_by {
                Some(result) => result.into(),
                None => Ok(Vec::new()),
            }
        })
    }

    /// Queries the module address prediction of the given Safe deployment data.
    pub fn query_module_address_prediction(
        &mut self,
        input: ModulePredictionInput,
    ) -> ClientResult<QueryHandle<ChainAddress>> {
        self.push(GraphQlQueries::query_module_address_prediction(input), |data| {
            data.calculate_module_address.into()
        })
    }

    /// Queries channel count and total wxHOPR balance matching the given selector.
    pub fn query_channel_stats(&mut self, selector: ChannelSelector) -> ClientResult<QueryHandle<ChannelStats>> {
        self.push(GraphQlQueries::query_channel_stats(selector), |data| {
            data.channel_stats.into()
        })
    }

    /// Queries the channels matching the given selector, including aggregated balance.
    ///
    /// Batched channel queries support the direct GraphQL selector fields only. A selector with `safe_address` is
    /// rejected because the single-query client currently applies additional client-side filtering that would require
    /// more than one GraphQL operation.
    pub fn query_channels(&mut self, selector: ChannelSelector) -> ClientResult<QueryHandle<ChannelsList>> {
        if selector.filter.is_none() && selector.safe_address.is_none() {
            return Err(ErrorKind::InvalidInput("at least one filter must be specified on channel query").into());
        }

        if selector.safe_address.is_some() {
            return Err(
                ErrorKind::InvalidInput("safe_address channel filtering is not supported in query batches").into(),
            );
        }

        self.push(GraphQlQueries::query_channels(selector), |data| data.channels.into())
    }

    /// Queries the status of a tracked transaction.
    pub fn query_transaction_status(&mut self, tx_id: TxId) -> ClientResult<QueryHandle<Transaction>> {
        self.push(GraphQlQueries::query_transaction(tx_id), |data| {
            data.transaction
                .ok_or::<BlokliClientError>(ErrorKind::NoData.into())?
                .into()
        })
    }

    /// Queries the chain info.
    pub fn query_chain_info(&mut self) -> ClientResult<QueryHandle<ChainInfo>> {
        self.push(GraphQlQueries::query_chain_info(), |data| data.chain_info.into())
    }

    /// Queries the version of the Blokli API.
    pub fn query_version(&mut self) -> ClientResult<QueryHandle<String>> {
        self.push(GraphQlQueries::query_version(), |data| Ok(data.version))
    }

    /// Queries the client compatibility contract exposed by the Blokli API.
    pub fn query_compatibility(&mut self) -> ClientResult<QueryHandle<Compatibility>> {
        self.push(GraphQlQueries::query_compatibility(), |data| Ok(data.compatibility))
    }

    /// Queries the health of the Blokli server.
    pub fn query_health(&mut self) -> ClientResult<QueryHandle<String>> {
        self.push(GraphQlQueries::query_health(), |data| Ok(data.health))
    }

    /// Queries the total wxHOPR balance held across indexed safe contracts.
    pub fn query_safes_balance(
        &mut self,
        owner_address: Option<ChainAddress>,
    ) -> ClientResult<QueryHandle<SafesBalance>> {
        self.push(GraphQlQueries::query_safes_balance(owner_address), |data| {
            data.safes_balance.into()
        })
    }

    /// Executes the queued queries in one HTTP batch request.
    ///
    /// Returns an error if the batch is empty, if the server response length does not match the queued query count, or
    /// if the underlying HTTP, GraphQL, compatibility, or deserialization step fails.
    pub async fn execute(self) -> ClientResult<BlokliQueryBatchResult> {
        if self.queries.is_empty() {
            return Err(ErrorKind::InvalidInput("query batch must contain at least one query").into());
        }

        let (operations, parsers): (Vec<_>, Vec<_>) = self
            .queries
            .into_iter()
            .map(|query| (query.operation, query.parser))
            .unzip();

        let responses = self.client.build_batch(operations)?.await?;
        if responses.len() != parsers.len() {
            return Err(ErrorKind::Batch("batch response length did not match request length").into());
        }

        let values = responses
            .into_iter()
            .zip(parsers)
            .map(|(response, parser)| Some(parser(response)))
            .collect();

        Ok(BlokliQueryBatchResult { values })
    }
}

/// Results returned by a query batch.
///
/// Use [`Self::take`] with the [`QueryHandle`] returned when the query was queued. Each result can only be taken once.
pub struct BlokliQueryBatchResult {
    values: Vec<Option<ClientResult<Box<dyn Any + Send>>>>,
}

impl BlokliQueryBatchResult {
    /// Takes the typed result for a query handle.
    ///
    /// Returns an error if the handle does not belong to this result set, if the value has already been taken, if the
    /// handle type does not match the stored result, or if the individual GraphQL response represented an error.
    pub fn take<T: Send + 'static>(&mut self, handle: QueryHandle<T>) -> ClientResult<T> {
        let slot = self
            .values
            .get_mut(handle.index)
            .ok_or(ErrorKind::Batch("query handle does not belong to this batch result"))?;
        let value = slot
            .take()
            .ok_or(ErrorKind::Batch("query batch result was already consumed"))??;

        value
            .downcast::<T>()
            .map(|value| *value)
            .map_err(|_| ErrorKind::Batch("query handle type did not match batch result").into())
    }
}
