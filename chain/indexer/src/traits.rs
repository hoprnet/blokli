use std::sync::Arc;

use async_trait::async_trait;
use blokli_chain_types::ContractAddresses;
use hopr_bindings::exports::alloy::primitives::B256;
use hopr_types::primitive::prelude::*;

use crate::errors::Result;

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
    /// This is the per-log primitive used by the default
    /// [`Self::collect_log_events`] implementation. Handlers may override the batch method to
    /// process a group atomically; in that case this method remains the fallback used for
    /// isolated retries.
    ///
    /// # Arguments
    /// * `log` - The blockchain log to process
    /// * `is_synced` - Whether the indexer has completed initial synchronization
    ///
    /// # Returns
    /// * `Result<()>` - Success or error
    async fn collect_log_event(&self, log: SerializableLog, is_synced: bool) -> Result<()>;

    /// Processes an ordered group of blockchain logs.
    ///
    /// The default invokes [`Self::collect_log_event`] sequentially and is not atomic.
    /// Implementations may override it to apply the complete group atomically, and must return
    /// `true` from [`Self::supports_atomic_batches`] when they do.
    async fn collect_log_events(&self, logs: Vec<SerializableLog>, is_synced: bool) -> Result<()> {
        for log in logs {
            self.collect_log_event(log, is_synced).await?;
        }

        Ok(())
    }

    /// Whether [`Self::collect_log_events`] applies its entire input atomically.
    ///
    /// The indexer marks a successful atomic batch as processed together, and may retry an
    /// unsuccessful atomic batch one log at a time. Handlers using the default sequential batch
    /// implementation must therefore keep this `false`: some earlier logs may already have
    /// applied side effects when a later log fails.
    fn supports_atomic_batches(&self) -> bool {
        false
    }

    /// Returns whether a fetched, canonical log should be dispatched to the contract handler.
    /// Removed logs are filtered by the indexer before this hook is called.
    fn should_process_log(&self, _log: &SerializableLog) -> bool {
        true
    }

    /// Reverts handler-owned derived state from the first affected block onward.
    async fn revert_block_derived_state(&self, _from_block: u64) -> Result<()> {
        Ok(())
    }
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
        async fn revert_block_derived_state(&self, from_block: u64) -> Result<()>;
    }
}
