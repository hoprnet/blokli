use std::{sync::Arc, time::Duration};

use async_trait::async_trait;
use blokli_chain_indexer::{IndexerConfig, block::Indexer, handlers::ContractEventHandlers, traits::ChainLogHandler};
use blokli_chain_rpc::{BlockWithLogs, FilterSet, HoprIndexerRpcOperations};
use blokli_chain_types::ContractAddresses;
use blokli_db::{api::logs::BlokliDbLogOperations, db::BlokliDb};
use futures::stream;
use hopr_crypto_types::types::Hash;
use hopr_primitive_types::prelude::*;
use tempfile::TempDir;

/// Mock RPC operations for testing
#[derive(Clone)]
struct MockRpcOperations {
    block_number: u64,
    hopr_balance: HoprBalance,
    xdai_balance: XDaiBalance,
}

impl MockRpcOperations {
    fn new() -> Self {
        Self {
            block_number: 2, // Set close to the blocks we'll provide (0, 1, 2)
            hopr_balance: HoprBalance::from(1000u64),
            xdai_balance: XDaiBalance::from(1000u64),
        }
    }
}

#[async_trait]
impl HoprIndexerRpcOperations for MockRpcOperations {
    async fn block_number(&self) -> blokli_chain_rpc::errors::Result<u64> {
        Ok(self.block_number)
    }

    async fn get_hopr_allowance(
        &self,
        _owner: Address,
        _spender: Address,
    ) -> blokli_chain_rpc::errors::Result<HoprBalance> {
        Ok(self.hopr_balance.clone())
    }

    async fn get_xdai_balance(&self, _address: Address) -> blokli_chain_rpc::errors::Result<XDaiBalance> {
        Ok(self.xdai_balance.clone())
    }

    async fn get_hopr_balance(&self, _address: Address) -> blokli_chain_rpc::errors::Result<HoprBalance> {
        Ok(self.hopr_balance.clone())
    }

    fn try_stream_logs<'a>(
        &'a self,
        start_block_number: u64,
        _filters: FilterSet,
        _is_synced: bool,
    ) -> blokli_chain_rpc::errors::Result<std::pin::Pin<Box<dyn futures::Stream<Item = BlockWithLogs> + Send + 'a>>>
    {
        use futures::stream::StreamExt;

        // Create a stream that emits a few blocks and then waits indefinitely
        // This prevents the stream from terminating immediately
        let blocks = vec![
            BlockWithLogs {
                block_id: start_block_number,
                ..Default::default()
            },
            BlockWithLogs {
                block_id: start_block_number + 1,
                ..Default::default()
            },
            BlockWithLogs {
                block_id: start_block_number + 2,
                ..Default::default()
            },
        ];

        // Create a stream that yields the blocks with delays and then ends
        let stream = stream::iter(blocks).then(|block| async move {
            tokio::time::sleep(Duration::from_millis(50)).await;
            block
        });

        Ok(Box::pin(stream))
    }
}

#[tokio::test]
async fn test_indexer_startup() -> anyhow::Result<()> {
    // Setup temporary directory for database
    let temp_dir = TempDir::new()?;
    let db_path = temp_dir.path();

    // Initialize database
    let db = BlokliDb::new_in_memory().await?;

    // Create mock RPC operations
    let mock_rpc = MockRpcOperations::new();

    // Create contract addresses with some non-zero values
    let contract_addresses = ContractAddresses {
        token: Address::from([1; 20]),
        channels: Address::from([2; 20]),
        announcements: Address::from([3; 20]),
        safe_registry: Address::from([6; 20]),
        price_oracle: Address::from([7; 20]),
        win_prob_oracle: Address::from([8; 20]),
        stake_factory: Address::from([9; 20]),
    };

    // Create indexer state for subscriptions (must be created before handlers)
    let indexer_state = blokli_chain_indexer::IndexerState::new(1000, 10);

    // Create event handlers
    let handlers = ContractEventHandlers::new(contract_addresses, db.clone(), mock_rpc.clone(), indexer_state.clone());

    // Initialize logs origin data using the proper contract addresses and topics
    let mut address_topics = vec![];
    for address in handlers.contract_addresses() {
        if address != handlers.contract_addresses_map().token {
            let topics = handlers.contract_address_topics(address);
            for topic in topics {
                address_topics.push((address, Hash::from(topic.0)));
            }
        }
    }
    db.ensure_logs_origin(address_topics).await?;

    // Configure indexer
    let indexer_config = IndexerConfig {
        start_block_number: 0,
        fast_sync: false, // Disable fast sync for testing
        enable_logs_snapshot: false,
        logs_snapshot_url: None,
        data_directory: db_path.to_string_lossy().to_string(),
        event_bus_capacity: 1000,
        shutdown_signal_capacity: 10,
    };

    // Create channel for events
    let (tx_events, _rx_events) = async_channel::unbounded();

    // Create indexer
    let indexer =
        Indexer::new(mock_rpc, handlers, db, indexer_config, tx_events, indexer_state).without_panic_on_completion(); // Don't panic when stream ends in tests

    // Start indexer
    let indexer_handle = indexer.start().await?;

    // Wait a bit for indexer to process blocks and potentially generate events
    tokio::time::sleep(Duration::from_millis(200)).await;

    // The indexer has started successfully if we got here without errors
    // Events may or may not be generated depending on the logs in the blocks

    // Abort indexer
    indexer_handle.abort();

    // Verify indexer was aborted successfully
    assert!(indexer_handle.is_aborted());

    Ok(())
}

#[tokio::test]
async fn test_indexer_with_fast_sync() -> anyhow::Result<()> {
    // Setup temporary directory for database
    let temp_dir = TempDir::new()?;
    let db_path = temp_dir.path();

    // Initialize database
    let db = BlokliDb::new_in_memory().await?;

    // Create mock RPC operations
    let mock_rpc = MockRpcOperations::new();

    // Create contract addresses with some non-zero values
    let contract_addresses = ContractAddresses {
        token: Address::from([1; 20]),
        channels: Address::from([2; 20]),
        announcements: Address::from([3; 20]),
        safe_registry: Address::from([6; 20]),
        price_oracle: Address::from([7; 20]),
        win_prob_oracle: Address::from([8; 20]),
        stake_factory: Address::from([9; 20]),
    };

    // Create indexer state for subscriptions (must be created before handlers)
    let indexer_state = blokli_chain_indexer::IndexerState::new(1000, 10);

    // Create event handlers
    let handlers = ContractEventHandlers::new(contract_addresses, db.clone(), mock_rpc.clone(), indexer_state.clone());

    // Initialize logs origin data using the proper contract addresses and topics
    let mut address_topics = vec![];
    for address in handlers.contract_addresses() {
        if address != handlers.contract_addresses_map().token {
            let topics = handlers.contract_address_topics(address);
            for topic in topics {
                address_topics.push((address, Hash::from(topic.0)));
            }
        }
    }
    db.ensure_logs_origin(address_topics).await?;

    // Configure indexer with fast sync enabled
    let indexer_config = IndexerConfig {
        start_block_number: 100, // Start from a later block
        fast_sync: true,
        enable_logs_snapshot: false, // Don't try to download snapshots
        logs_snapshot_url: None,
        data_directory: db_path.to_string_lossy().to_string(),
        event_bus_capacity: 1000,
        shutdown_signal_capacity: 10,
    };

    // Create channel for events
    let (tx_events, _rx_events) = async_channel::unbounded();

    // Create indexer
    let indexer =
        Indexer::new(mock_rpc, handlers, db, indexer_config, tx_events, indexer_state).without_panic_on_completion();

    // Start indexer
    let indexer_handle = indexer.start().await?;

    // Let it run briefly
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Abort indexer
    indexer_handle.abort();

    Ok(())
}

#[tokio::test]
async fn test_indexer_handles_start_block_configuration() -> anyhow::Result<()> {
    // Setup temporary directory for database
    let temp_dir = TempDir::new()?;
    let db_path = temp_dir.path();

    // Initialize database
    let db = BlokliDb::new_in_memory().await?;

    // Create a custom mock RPC that tracks the start block requested
    #[derive(Clone)]
    struct TrackingMockRpc {
        inner: MockRpcOperations,
        requested_start_block: Arc<tokio::sync::Mutex<Option<u64>>>,
    }

    #[async_trait]
    impl HoprIndexerRpcOperations for TrackingMockRpc {
        async fn block_number(&self) -> blokli_chain_rpc::errors::Result<u64> {
            self.inner.block_number().await
        }

        async fn get_hopr_allowance(
            &self,
            owner: Address,
            spender: Address,
        ) -> blokli_chain_rpc::errors::Result<HoprBalance> {
            self.inner.get_hopr_allowance(owner, spender).await
        }

        async fn get_xdai_balance(&self, address: Address) -> blokli_chain_rpc::errors::Result<XDaiBalance> {
            self.inner.get_xdai_balance(address).await
        }

        async fn get_hopr_balance(&self, address: Address) -> blokli_chain_rpc::errors::Result<HoprBalance> {
            self.inner.get_hopr_balance(address).await
        }

        fn try_stream_logs<'a>(
            &'a self,
            start_block_number: u64,
            _filters: FilterSet,
            _is_synced: bool,
        ) -> blokli_chain_rpc::errors::Result<std::pin::Pin<Box<dyn futures::Stream<Item = BlockWithLogs> + Send + 'a>>>
        {
            use futures::stream::StreamExt;

            // Record the requested start block
            let requested_start = self.requested_start_block.clone();
            tokio::spawn(async move {
                *requested_start.lock().await = Some(start_block_number);
            });

            // Create a stream that emits a few blocks and then waits indefinitely
            let blocks = vec![
                BlockWithLogs {
                    block_id: start_block_number,
                    ..Default::default()
                },
                BlockWithLogs {
                    block_id: start_block_number + 1,
                    ..Default::default()
                },
                BlockWithLogs {
                    block_id: start_block_number + 2,
                    ..Default::default()
                },
            ];

            // Create a stream that yields the blocks with delays and then ends
            let stream = stream::iter(blocks).then(|block| async move {
                tokio::time::sleep(Duration::from_millis(50)).await;
                block
            });

            Ok(Box::pin(stream))
        }
    }

    let requested_start_block = Arc::new(tokio::sync::Mutex::new(None));
    let tracking_rpc = TrackingMockRpc {
        inner: MockRpcOperations::new(),
        requested_start_block: requested_start_block.clone(),
    };

    // Create contract addresses with some non-zero values
    let contract_addresses = ContractAddresses {
        token: Address::from([1; 20]),
        channels: Address::from([2; 20]),
        announcements: Address::from([3; 20]),
        safe_registry: Address::from([6; 20]),
        price_oracle: Address::from([7; 20]),
        win_prob_oracle: Address::from([8; 20]),
        stake_factory: Address::from([9; 20]),
    };

    // Create indexer state for subscriptions (must be created before handlers)
    let indexer_state = blokli_chain_indexer::IndexerState::new(1000, 10);

    // Create event handlers
    let handlers = ContractEventHandlers::new(
        contract_addresses,
        db.clone(),
        tracking_rpc.clone(),
        indexer_state.clone(),
    );

    // Initialize logs origin data using the proper contract addresses and topics
    let mut address_topics = vec![];
    for address in handlers.contract_addresses() {
        if address != handlers.contract_addresses_map().token {
            let topics = handlers.contract_address_topics(address);
            for topic in topics {
                address_topics.push((address, Hash::from(topic.0)));
            }
        }
    }
    db.ensure_logs_origin(address_topics).await?;

    // Configure indexer with specific start block
    let expected_start_block = 500u64;
    let indexer_config = IndexerConfig {
        start_block_number: expected_start_block,
        fast_sync: false,
        enable_logs_snapshot: false,
        logs_snapshot_url: None,
        data_directory: db_path.to_string_lossy().to_string(),
        event_bus_capacity: 1000,
        shutdown_signal_capacity: 10,
    };

    // Create channel for events
    let (tx_events, _rx_events) = async_channel::unbounded();

    // Create and start indexer
    let indexer = Indexer::new(tracking_rpc, handlers, db, indexer_config, tx_events, indexer_state)
        .without_panic_on_completion();

    let indexer_handle = indexer.start().await?;

    // Wait for the indexer to start processing
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Check that the correct start block was requested
    let actual_start = *requested_start_block.lock().await;
    assert_eq!(
        actual_start,
        Some(expected_start_block),
        "Indexer should request logs starting from configured block"
    );

    // Cleanup
    indexer_handle.abort();

    Ok(())
}
