use std::sync::Arc;

use async_trait::async_trait;
use blokli_chain_types::ContractAddresses;
use blokli_db::OpenTransaction;
use hopr_bindings::exports::alloy::primitives::B256;
use hopr_types::primitive::prelude::*;

use crate::{errors::Result, state::IndexerEvent};

#[async_trait]
pub trait ChainLogHandler {
    fn contract_addresses(&self) -> Vec<Address>;

    /// Returns the mapping of contract types to their deployed addresses.
    ///
    /// This method provides access to the configuration that maps logical
    /// contract roles (token, channels, registry, etc.) to their actual
    /// deployed Ethereum addresses.
    ///
    /// # Returns
    /// * `&ContractAddresses` - Reference to the contract addresses configuration
    fn contract_addresses_map(&self) -> Arc<ContractAddresses>;

    /// Returns the event signature topics for efficient log filtering.
    ///
    /// This method provides the event signatures (topics) that should be
    /// monitored for a given contract address, enabling efficient blockchain
    /// log filtering by combining address and topic filters.
    ///
    /// # Arguments
    /// * `contract` - The contract address to get event topics for
    ///
    /// # Returns
    /// * `Vec<B256>` - Vector of event signature hashes (topics) for the contract
    fn contract_address_topics(&self, contract: Address) -> Vec<B256>;

    /// Processes a single blockchain log.
    ///
    /// This method processes individual blockchain logs, replacing the previous
    /// batch processing approach for better error isolation and granular control.
    /// Events are published internally via the IndexerState event bus.
    ///
    /// # Arguments
    /// * `log` - The blockchain log to process
    /// * `is_synced` - Whether the indexer has completed initial synchronization
    ///
    /// # Returns
    /// * `Result<()>` - Success or error
    async fn collect_log_event(&self, log: SerializableLog, is_synced: bool) -> Result<()>;

    async fn collect_log_event_in_tx(
        &self,
        tx: &OpenTransaction,
        log: SerializableLog,
        is_synced: bool,
    ) -> Result<Vec<IndexerEvent>>;

    async fn clear_pending_safe_mutations(&self, tx: &OpenTransaction);
    async fn clear_pending_safe_redeemed_stats(&self, tx: &OpenTransaction);

    async fn merge_pending_safe_mutations(&self, from_key: usize, tx: &OpenTransaction);
    async fn merge_pending_safe_redeemed_stats(&self, from_key: usize, tx: &OpenTransaction);

    async fn flush_pending_safe_mutations(&self, tx: &OpenTransaction) -> Result<()>;
    async fn flush_pending_safe_redeemed_stats(&self, tx: &OpenTransaction) -> Result<()>;

    async fn discard_pending_safe_mutations(&self, tx: &OpenTransaction);
    async fn discard_pending_safe_redeemed_stats(&self, tx: &OpenTransaction);
}

#[cfg(test)]
use mockall::mock;

#[cfg(test)]
mock! {
    /// Mock implementation of ChainLogHandler for testing.
    ///
    /// # Example
    /// ```
    /// use mockall::predicate::*;
    /// let mut mock = MockChainLogHandler::new();
    /// mock.expect_collect_log_event()
    ///     .returning(|_, _| Ok(None));
    /// ```
    pub ChainLogHandler {}

    impl Clone for ChainLogHandler {
        fn clone(&self) -> Self;
    }

    #[async_trait]
    impl ChainLogHandler for ChainLogHandler {
        fn contract_addresses(&self) -> Vec<Address>;
        fn contract_addresses_map(&self) -> Arc<ContractAddresses>;
        fn contract_address_topics(&self, contract: Address) -> Vec<B256>;
        async fn collect_log_event(&self, log: SerializableLog, is_synced: bool) -> Result<()>;
        async fn collect_log_event_in_tx(
            &self,
            tx: &OpenTransaction,
            log: SerializableLog,
            is_synced: bool,
        ) -> Result<Vec<IndexerEvent>>;
        async fn clear_pending_safe_mutations(&self, tx: &OpenTransaction);
        async fn clear_pending_safe_redeemed_stats(&self, tx: &OpenTransaction);
        async fn merge_pending_safe_mutations(&self, from_key: usize, tx: &OpenTransaction);
        async fn merge_pending_safe_redeemed_stats(&self, from_key: usize, tx: &OpenTransaction);
        async fn flush_pending_safe_mutations(&self, tx: &OpenTransaction) -> Result<()>;
        async fn flush_pending_safe_redeemed_stats(&self, tx: &OpenTransaction) -> Result<()>;
        async fn discard_pending_safe_mutations(&self, tx: &OpenTransaction);
        async fn discard_pending_safe_redeemed_stats(&self, tx: &OpenTransaction);
    }
}
