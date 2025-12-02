use std::{
    collections::HashSet,
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
};

use alloy::{primitives::Address as AlloyAddress, sol_types::SolEvent};
use blokli_chain_rpc::{BlockWithLogs, FilterSet, HoprIndexerRpcOperations};
use blokli_chain_types::AlloyAddressExt;
use blokli_db::{
    BlokliDbGeneralModelOperations, TargetDb, api::logs::BlokliDbLogOperations, info::BlokliDbInfoOperations,
};
use blokli_db_entity::{channel_state, prelude::ChannelState};
use futures::{StreamExt, future::AbortHandle};
use hopr_bindings::hopr_token::HoprToken::{Approval, Transfer};
use hopr_crypto_types::types::Hash;
use hopr_primitive_types::prelude::{Address, SerializableLog};
use sea_orm::{ActiveModelTrait, ActiveValue::Set, ColumnTrait, Condition, EntityTrait, QueryFilter, QueryOrder};
use tracing::{debug, error, info, trace};

use crate::{
    IndexerConfig, IndexerState,
    errors::{CoreEthereumIndexerError, Result},
    snapshot::{SnapshotInfo, SnapshotManager},
    traits::ChainLogHandler,
};

#[cfg(all(feature = "prometheus", not(test)))]
lazy_static::lazy_static! {
    static ref METRIC_INDEXER_CURRENT_BLOCK: hopr_metrics::metrics::SimpleGauge =
        hopr_metrics::metrics::SimpleGauge::new(
            "hopr_indexer_block_number",
            "Current last processed block number by the indexer",
    ).unwrap();
    static ref METRIC_INDEXER_CHECKSUM: hopr_metrics::metrics::SimpleGauge =
        hopr_metrics::metrics::SimpleGauge::new(
            "hopr_indexer_checksum",
            "Contains an unsigned integer that represents the low 32-bits of the Indexer checksum"
    ).unwrap();
    static ref METRIC_INDEXER_SYNC_PROGRESS: hopr_metrics::metrics::SimpleGauge =
        hopr_metrics::metrics::SimpleGauge::new(
            "hopr_indexer_sync_progress",
            "Sync progress of the historical data by the indexer",
    ).unwrap();
    static ref METRIC_INDEXER_SYNC_SOURCE: hopr_metrics::metrics::MultiGauge =
        hopr_metrics::metrics::MultiGauge::new(
            "hopr_indexer_data_source",
            "Current data source of the Indexer",
            &["source"],
    ).unwrap();

}

/// Information about a detected blockchain reorganization.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReorgInfo {
    /// The block number where the reorg was detected
    pub detected_at_block: u64,
    /// The range of blocks affected by the reorg (min, max)
    pub affected_block_range: (u64, u64),
    /// Number of logs that were marked as removed
    pub removed_log_count: usize,
}

/// Indexer
///
/// Accepts the RPC operational functionality [blokli_chain_rpc::HoprIndexerRpcOperations]
/// and provides the indexing operation resulting in and output of
/// [blokli_chain_types::chain_events::SignificantChainEvent] streamed outside the indexer by the unbounded channel.
///
/// The roles of the indexer:
/// 1. prime the RPC endpoint
/// 2. request an RPC stream of changes to process
/// 3. process block and log stream
/// 4. ensure finalization by postponing processing until the head is far enough
/// 5. store relevant data into the DB
/// 6. pass the processing on to the business logic
#[derive(Debug, Clone)]
pub struct Indexer<T, U, Db>
where
    T: HoprIndexerRpcOperations + Send + 'static,
    U: ChainLogHandler + Send + 'static,
    Db: BlokliDbGeneralModelOperations + BlokliDbInfoOperations + BlokliDbLogOperations + Clone + Send + Sync + 'static,
{
    rpc: Option<T>,
    db_processor: Option<U>,
    db: Db,
    cfg: IndexerConfig,
    indexer_state: IndexerState,
    // If true (default), the indexer will panic if the event stream is terminated.
    // Setting it to false is useful for testing.
    panic_on_completion: bool,
}

impl<T, U, Db> Indexer<T, U, Db>
where
    T: HoprIndexerRpcOperations + Sync + Send + 'static,
    U: ChainLogHandler + Send + Sync + 'static,
    Db: BlokliDbGeneralModelOperations + BlokliDbInfoOperations + BlokliDbLogOperations + Clone + Send + Sync + 'static,
{
    pub fn new(rpc: T, db_processor: U, db: Db, cfg: IndexerConfig, indexer_state: IndexerState) -> Self {
        Self {
            rpc: Some(rpc),
            db_processor: Some(db_processor),
            db,
            cfg,
            indexer_state,
            panic_on_completion: true,
        }
    }

    /// Disables the panic on completion.
    pub fn without_panic_on_completion(mut self) -> Self {
        self.panic_on_completion = false;
        self
    }

    pub async fn start(mut self) -> Result<AbortHandle>
    where
        T: HoprIndexerRpcOperations + 'static,
        U: ChainLogHandler + 'static,
        Db: BlokliDbGeneralModelOperations
            + BlokliDbInfoOperations
            + BlokliDbLogOperations
            + Clone
            + Send
            + Sync
            + 'static,
    {
        if self.rpc.is_none() || self.db_processor.is_none() {
            return Err(CoreEthereumIndexerError::ProcessError(
                "indexer cannot start, missing components".into(),
            ));
        }

        info!("Starting chain indexing");

        let rpc = self.rpc.take().expect("rpc should be present");
        let logs_handler = Arc::new(self.db_processor.take().expect("db_processor should be present"));
        let db = self.db.clone();
        let panic_on_completion = self.panic_on_completion;

        let (log_filters, address_topics) = Self::generate_log_filters(&logs_handler);

        // Check that the contract addresses and topics are consistent with what is in the logs DB,
        // or if the DB is empty, prime it with the given addresses and topics.
        db.ensure_logs_origin(address_topics).await?;

        let is_synced = Arc::new(AtomicBool::new(false));
        let chain_head = Arc::new(AtomicU64::new(0));

        // update the chain head once at startup to get a reference for initial syncing
        // progress calculation
        debug!("Updating chain head at indexer startup");
        Self::update_chain_head(&rpc, chain_head.clone()).await;

        // First, check whether fast sync is enabled and can be performed.
        // If so:
        //   1. Download the snapshot if the logs database is empty and the snapshot is enabled
        //   2. Delete the existing indexed data
        //   3. Reset fast sync progress
        //   4. Run the fast sync process until completion
        //   5. Finally, starting the rpc indexer.
        let fast_sync_configured = self.cfg.fast_sync;
        let index_empty = self.db.index_is_empty().await?;

        // Pre-start operations to ensure the indexer is ready, including snapshot fetching
        self.pre_start().await?;

        #[derive(PartialEq, Eq)]
        enum FastSyncMode {
            None,
            FromScratch,
            Continue,
        }

        let will_perform_fast_sync = match (fast_sync_configured, index_empty) {
            (true, false) => {
                info!(
                    "Fast sync is enabled, but the index database is not empty. Fast sync will continue on existing \
                     unprocessed logs."
                );
                FastSyncMode::Continue
            }
            (false, true) => {
                info!("Fast sync is disabled, but the index database is empty. Doing a full re-sync.");
                // Clean the last processed log from the Log DB, to allow full resync
                self.db.clear_index_db(None).await?;
                self.db.set_logs_unprocessed(None, None).await?;
                FastSyncMode::None
            }
            (false, false) => {
                info!("Fast sync is disabled and the index database is not empty. Continuing normal sync.");
                FastSyncMode::None
            }
            (true, true) => {
                info!("Fast sync is enabled, starting the fast sync process");
                // To ensure a proper state, reset any auxiliary data in the database
                self.db.clear_index_db(None).await?;
                self.db.set_logs_unprocessed(None, None).await?;
                FastSyncMode::FromScratch
            }
        };

        let (tx, mut rx) = futures::channel::mpsc::channel::<()>(1);

        // Perform the fast-sync if requested
        if FastSyncMode::None != will_perform_fast_sync {
            let processed = match will_perform_fast_sync {
                FastSyncMode::FromScratch => None,
                FastSyncMode::Continue => Some(false),
                _ => unreachable!(),
            };

            #[cfg(all(feature = "prometheus", not(test)))]
            {
                METRIC_INDEXER_SYNC_SOURCE.set(&["fast-sync"], 1.0);
                METRIC_INDEXER_SYNC_SOURCE.set(&["rpc"], 0.0);
            }

            let log_block_numbers = self.db.get_logs_block_numbers(None, None, processed).await?;
            let _first_log_block_number = log_block_numbers.first().copied().unwrap_or(0);
            let _head = chain_head.load(Ordering::Relaxed);
            for block_number in log_block_numbers {
                debug!(
                    block_number,
                    first_log_block_number = _first_log_block_number,
                    head = _head,
                    "computing processed logs"
                );
                // Do not pollute the logs with the fast-sync progress
                Self::process_block_by_id(
                    &db,
                    &logs_handler,
                    block_number,
                    is_synced.load(Ordering::Relaxed),
                    &self.indexer_state,
                )
                .await?;

                #[cfg(all(feature = "prometheus", not(test)))]
                {
                    let progress =
                        (block_number - _first_log_block_number) as f64 / (_head - _first_log_block_number) as f64;
                    METRIC_INDEXER_SYNC_PROGRESS.set(progress);
                }
            }
        }

        info!("Building rpc indexer background process");

        let next_block_to_process = if let Some(last_log) = self.db.get_last_checksummed_log().await? {
            info!(
                start_block = last_log.block_number,
                start_checksum = last_log.checksum.unwrap(),
                "Loaded indexer state",
            );

            if self.cfg.start_block_number < last_log.block_number {
                // If some prior indexing took place already, avoid reprocessing
                last_log.block_number + 1
            } else {
                self.cfg.start_block_number
            }
        } else {
            self.cfg.start_block_number
        };

        info!(next_block_to_process, "Indexer start point");

        let indexer_state = self.indexer_state.clone();
        let indexing_abort_handle = hopr_async_runtime::spawn_as_abortable!(async move {
            // Update the chain head once again
            debug!("Updating chain head at indexer startup");
            Self::update_chain_head(&rpc, chain_head.clone()).await;

            #[cfg(all(feature = "prometheus", not(test)))]
            {
                METRIC_INDEXER_SYNC_SOURCE.set(&["fast-sync"], 0.0);
                METRIC_INDEXER_SYNC_SOURCE.set(&["rpc"], 1.0);
            }

            let rpc_ref = &rpc;

            let event_stream = rpc
                .try_stream_logs(next_block_to_process, log_filters, is_synced.load(Ordering::Relaxed))
                .expect("block stream should be constructible")
                .then(|block| {
                    let db = db.clone();
                    let chain_head = chain_head.clone();
                    let is_synced = is_synced.clone();
                    let tx = tx.clone();
                    let logs_handler = logs_handler.clone();

                    async move {
                        Self::calculate_sync_process(
                            block.block_id,
                            rpc_ref,
                            db,
                            chain_head.clone(),
                            is_synced.clone(),
                            next_block_to_process,
                            tx.clone(),
                            logs_handler.contract_addresses_map().channels.into(),
                        )
                        .await;

                        block
                    }
                })
                .filter_map(|block| {
                    let db = db.clone();
                    let logs_handler = logs_handler.clone();

                    async move {
                        debug!(%block, "storing logs from block");
                        let logs = block.logs.clone();

                        // Filter out the token contract logs because we do not need to store these
                        // in the database.
                        let logs_vec = logs
                            .into_iter()
                            .filter(|log| log.address != logs_handler.contract_addresses_map().token)
                            .collect();

                        match db.store_logs(logs_vec).await {
                            Ok(store_results) => {
                                if let Some(error) = store_results
                                    .into_iter()
                                    .filter(|r| r.is_err())
                                    .map(|r| r.unwrap_err())
                                    .next()
                                {
                                    error!(%block, %error, "failed to processed stored logs from block");
                                    None
                                } else {
                                    Some(block)
                                }
                            }
                            Err(error) => {
                                error!(%block, %error, "failed to store logs from block");
                                None
                            }
                        }
                    }
                });

            let block_processing_stream = event_stream.then(|block| {
                let db = db.clone();
                let logs_handler = logs_handler.clone();
                let is_synced = is_synced.clone();
                let indexer_state = indexer_state.clone();
                async move {
                    // Events are now published directly via IndexerState within handlers
                    Self::process_block(
                        &db,
                        &logs_handler,
                        block,
                        false,
                        is_synced.load(Ordering::Relaxed),
                        &indexer_state,
                    )
                    .await;
                }
            });

            // Process all blocks (events are published internally via IndexerState)
            block_processing_stream.collect::<Vec<_>>().await;

            if panic_on_completion {
                panic!(
                    "Indexer event stream has been terminated. This error may be caused by a failed RPC connection."
                );
            }
        });

        if rx.next().await.is_some() {
            Ok(indexing_abort_handle)
        } else {
            Err(crate::errors::CoreEthereumIndexerError::ProcessError(
                "Error during indexing start".into(),
            ))
        }
    }

    pub async fn pre_start(&self) -> Result<()> {
        let fast_sync_configured = self.cfg.fast_sync;
        let index_empty = self.db.index_is_empty().await?;

        // Check if we need to download snapshot before fast sync
        let logs_db_has_data = self.has_logs_data().await?;

        if fast_sync_configured && index_empty && !logs_db_has_data && self.cfg.enable_logs_snapshot {
            info!("Logs database is empty, attempting to download logs snapshot...");

            match self.download_snapshot().await {
                Ok(snapshot_info) => {
                    info!("Logs snapshot downloaded successfully: {:?}", snapshot_info);
                }
                Err(e) => {
                    error!("Failed to download logs snapshot: {}. Continuing with regular sync.", e);
                }
            }
        }

        Ok(())
    }

    /// Generates specialized log filters for efficient blockchain event processing.
    ///
    /// This function creates a comprehensive set of log filters that optimize
    /// indexer performance by categorizing filters based on contract types and
    /// event relevance to the specific node.
    ///
    /// # Arguments
    /// * `logs_handler` - Handler containing contract addresses and safe address
    ///
    /// # Returns
    /// * `(FilterSet, Vec<(Address, Hash)>)` - A tuple containing:
    ///   - `FilterSet`: Categorized filters for blockchain event processing.
    ///   - `Vec<(Address, Hash)>`: A vector of address-topic pairs for logs origin validation.
    ///
    /// # Filter Categories
    /// * `all` - Complete set of filters for normal operation
    /// * `token` - Token-specific filters (Transfer, Approval events for safe)
    /// * `no_token` - Non-token contract filters for initial sync optimization
    fn generate_log_filters(logs_handler: &U) -> (FilterSet, Vec<(Address, Hash)>) {
        let addresses_no_token = logs_handler
            .contract_addresses()
            .into_iter()
            .filter(|a| *a != logs_handler.contract_addresses_map().token)
            .collect::<Vec<_>>();
        let mut filter_base_addresses = vec![];
        let mut filter_base_topics = vec![];
        let mut address_topics = vec![];

        addresses_no_token.iter().for_each(|address| {
            let topics = logs_handler.contract_address_topics(*address);
            if !topics.is_empty() {
                filter_base_addresses.push(AlloyAddress::from_hopr_address(*address));
                filter_base_topics.extend(topics.clone());
                for topic in topics.iter() {
                    address_topics.push((*address, Hash::from(topic.0)))
                }
            }
        });

        let filter_base = alloy::rpc::types::Filter::new()
            .address(filter_base_addresses)
            .event_signature(filter_base_topics);
        let filter_token = alloy::rpc::types::Filter::new().address(AlloyAddress::from_hopr_address(
            logs_handler.contract_addresses_map().token,
        ));

        let filter_token_transfer = filter_token.clone().event_signature(Transfer::SIGNATURE_HASH);

        let filter_token_approval = filter_token.event_signature(Approval::SIGNATURE_HASH);

        let set = FilterSet {
            all: vec![
                filter_base.clone(),
                filter_token_transfer.clone(),
                filter_token_approval.clone(),
            ],
            // token: vec![filter_transfer_from, filter_transfer_to, filter_approval],
            token: vec![filter_token_approval, filter_token_transfer],
            no_token: vec![filter_base],
        };

        (set, address_topics)
    }

    /// Processes a block by its ID.
    ///
    /// This function retrieves logs for the given block ID and processes them using the database
    /// and log handler.
    ///
    /// # Arguments
    ///
    /// * `db` - The database operations handler.
    /// * `logs_handler` - The database log handler.
    /// * `block_id` - The ID of the block to process.
    ///
    /// # Returns
    ///
    /// A `Result` containing an Option with unit type if the operation succeeds or an error if it fails.
    async fn process_block_by_id(
        db: &Db,
        logs_handler: &U,
        block_id: u64,
        is_synced: bool,
        indexer_state: &IndexerState,
    ) -> crate::errors::Result<Option<()>>
    where
        U: ChainLogHandler + 'static,
        Db: BlokliDbLogOperations + 'static,
    {
        let logs = db.get_logs(Some(block_id), Some(0)).await?;
        let mut block = BlockWithLogs {
            block_id,
            ..Default::default()
        };

        for log in logs {
            if log.block_number == block_id {
                block.logs.insert(log);
            } else {
                error!(
                    expected = block_id,
                    actual = log.block_number,
                    "block number mismatch in logs from database"
                );
                panic!("block number mismatch in logs from database")
            }
        }

        Ok(Self::process_block(db, logs_handler, block, true, is_synced, indexer_state).await)
    }

    /// Processes a block and its logs.
    ///
    /// This function processes the block logs and updates the database.
    /// Events are published internally via IndexerState event bus.
    ///
    /// # Arguments
    ///
    /// * `db` - The database operations handler.
    /// * `logs_handler` - The database log handler.
    /// * `block` - The block with logs to process.
    /// * `fetch_checksum_from_db` - A boolean indicating whether to fetch the checksum from the database.
    ///
    /// # Returns
    ///
    /// An Option with unit type if the operation succeeds.
    async fn process_block(
        db: &Db,
        logs_handler: &U,
        block: BlockWithLogs,
        fetch_checksum_from_db: bool,
        is_synced: bool,
        indexer_state: &IndexerState,
    ) -> Option<()>
    where
        U: ChainLogHandler + 'static,
        Db: BlokliDbLogOperations + 'static,
    {
        let block_id = block.block_id;
        let log_count = block.logs.len();
        debug!(block_id, "processing events");

        // Check for blockchain reorganization before processing logs
        if let Some(reorg_info) = Self::detect_reorg(&block) {
            error!(
                block_id = reorg_info.detected_at_block,
                affected_blocks = ?reorg_info.affected_block_range,
                removed_logs = reorg_info.removed_log_count,
                "Blockchain reorganization detected"
            );

            // Handle the reorg by inserting corrective channel states
            // Use the current block as the canonical block for corrective states
            match Self::handle_reorg(db, &reorg_info, block_id, indexer_state).await {
                Ok(corrected_count) => {
                    info!(
                        block_id,
                        corrected_channels = corrected_count,
                        "Successfully handled blockchain reorganization"
                    );
                }
                Err(error) => {
                    error!(
                        block_id,
                        %error,
                        "Failed to handle blockchain reorganization, panicking to prevent data corruption"
                    );
                    panic!("Failed to handle blockchain reorganization: {error}");
                }
            }
        }

        // FIXME: The block indexing and marking as processed should be done in a single
        // transaction. This is difficult since currently this would be across databases.
        // Process all logs - events are published internally via IndexerState
        for log in block.logs.clone() {
            match logs_handler.collect_log_event(log.clone(), is_synced).await {
                Ok(()) => match db.set_log_processed(log).await {
                    Ok(_) => {}
                    Err(error) => {
                        error!(block_id, %error, "failed to mark log as processed, panicking to prevent data loss");
                        panic!("failed to mark log as processed, panicking to prevent data loss")
                    }
                },
                Err(CoreEthereumIndexerError::ProcessError(error)) => {
                    error!(block_id, %error, "failed to process log, continuing indexing");
                }
                Err(error) => {
                    error!(block_id, %error, "failed to process log, panicking to prevent data loss");
                    panic!("failed to process log, panicking to prevent data loss")
                }
            }
        }

        // if we made it this far, no errors occurred and we can update checksums and indexer state
        match db.update_logs_checksums().await {
            Ok(last_log_checksum) => {
                let checksum = if fetch_checksum_from_db {
                    let last_log = block.logs.into_iter().next_back()?;
                    let log = db.get_log(block_id, last_log.tx_index, last_log.log_index).await.ok()?;

                    log.checksum?
                } else {
                    last_log_checksum.to_string()
                };

                if log_count != 0 {
                    info!(
                        block_number = block_id,
                        log_count, last_log_checksum = ?checksum, "Indexer state update",
                    );

                    #[cfg(all(feature = "prometheus", not(test)))]
                    {
                        if let Ok(checksum_hash) = Hash::from_hex(checksum.as_str()) {
                            let low_4_bytes =
                                hopr_primitive_types::prelude::U256::from_big_endian(checksum_hash.as_ref()).low_u32();
                            METRIC_INDEXER_CHECKSUM.set(low_4_bytes.into());
                        } else {
                            error!("Invalid checksum generated from logs");
                        }
                    }
                }

                // finally update the block number in the database to the last processed block
                match db.set_indexer_state_info(None, block_id as u32).await {
                    Ok(_) => {
                        trace!(block_id, "updated indexer state info");
                    }
                    Err(error) => error!(block_id, %error, "failed to update indexer state info"),
                }
            }
            Err(error) => error!(block_id, %error, "failed to update checksums for logs from block"),
        }

        debug!(block_id, "processed block logs - events published via IndexerState",);

        Some(())
    }

    /// Detects blockchain reorganization by checking for removed logs in a block.
    ///
    /// This function scans logs in a block to identify if any have been marked as removed
    /// (indicating they were part of a reorganized chain). When a reorg is detected,
    /// it returns information about which blocks and channels are affected.
    ///
    /// # Arguments
    ///
    /// * `block` - The block with logs to check for reorg indicators
    ///
    /// # Returns
    ///
    /// * `Option<ReorgInfo>` - Information about the detected reorg, or None if no reorg was detected
    fn detect_reorg(block: &BlockWithLogs) -> Option<ReorgInfo> {
        let removed_logs: Vec<&SerializableLog> = block.logs.iter().filter(|log| log.removed).collect();

        if removed_logs.is_empty() {
            return None;
        }

        let affected_blocks: Vec<u64> = removed_logs.iter().map(|log| log.block_number).collect();
        let min_block = affected_blocks.iter().min().copied().unwrap_or(block.block_id);
        let max_block = affected_blocks.iter().max().copied().unwrap_or(block.block_id);

        Some(ReorgInfo {
            detected_at_block: block.block_id,
            affected_block_range: (min_block, max_block),
            removed_log_count: removed_logs.len(),
        })
    }

    /// Handle blockchain reorganization by inserting corrective states
    ///
    /// When a blockchain reorg occurs, this function preserves the audit trail by inserting
    /// **corrective states** that restore affected channels to their pre-reorg state, without
    /// deleting any historical data.
    ///
    /// # Algorithm
    ///
    /// 1. **Identify affected channels**: Query all channel states in the affected block range
    /// 2. **Find last valid state**: For each affected channel, locate the most recent state before the reorg occurred
    ///    (the "watermark" state)
    /// 3. **Insert corrective state**: Create a new state at synthetic position `(canonical_block, 0, 0)` with the
    ///    watermark state's values and `reorg_correction = true`
    /// 4. **Skip channels without prior state**: Channels opened during the reorg are skipped (they will be re-indexed
    ///    with the canonical chain)
    ///
    /// # Corrective States
    ///
    /// Corrective states are special state records that:
    /// - Are inserted at **synthetic positions** `(canonical_block, 0, 0)` where `canonical_block` is the first valid
    ///   block after the reorg
    /// - Have the `reorg_correction` flag set to `true` to distinguish them from normal states
    /// - Contain the same state values (balance, status, etc.) as the last valid state before the reorg
    /// - Preserve the complete audit trail - no data is ever deleted
    ///
    /// The synthetic position `(block, 0, 0)` ensures corrective states are ordered before any
    /// real events at the same block (which have `tx_index > 0` or `log_index > 0`).
    ///
    /// # Audit Trail Preservation
    ///
    /// This function follows the **never-delete principle**:
    /// - Invalidated states from reorganized blocks remain in the database
    /// - Temporal queries automatically use corrective states to reflect canonical chain state
    /// - Complete history is preserved for auditing and forensic analysis
    /// - Multiple reorgs can occur - each adds new corrective states without removing old ones
    ///
    /// # Arguments
    ///
    /// * `db` - Database connection implementing required operations
    /// * `reorg_info` - Information about the detected reorganization:
    ///   - `affected_block_range`: `(min_block, max_block)` range that was reorganized
    ///   - `removed_log_count`: Number of logs removed during the reorg (for metrics)
    /// * `canonical_block` - The first valid block number on the canonical chain after the reorg. This is where
    ///   corrective states are positioned.
    ///
    /// # Returns
    ///
    /// Returns `Ok(count)` where `count` is the number of channels that received corrective states.
    ///
    /// A count of 0 indicates either:
    /// - No channels were affected by the reorg
    /// - All affected channels were opened during the reorg (no prior state to restore)
    ///
    /// # Errors
    ///
    /// Returns `CoreEthereumIndexerError::ProcessError` if:
    /// - Database queries fail while identifying affected channels
    /// - Cannot query for last valid states before the reorg
    /// - Inserting corrective states into the database fails
    ///
    /// On error, the function may have partially completed. Some channels may have received
    /// corrective states while others have not. The indexer should be restarted to retry.
    ///
    /// # Edge Cases
    ///
    /// - **Channels opened during reorg**: Skipped (no prior state to restore)
    /// - **Multiple states at same block**: Uses lexicographic ordering to find the correct watermark
    /// - **Reorg at block boundaries**: Correctly handles inclusive/exclusive range logic
    /// - **Repeated reorgs**: Each creates new corrective states; all history is preserved
    /// - **Large reorgs**: Efficiently processes 100+ affected blocks with minimal overhead
    ///
    /// # Performance
    ///
    /// - Scales linearly with the number of affected channels, not the number of blocks
    /// - Uses database indices for efficient state lookups
    /// - Tested with reorgs affecting 50+ channels and 100+ blocks
    ///
    /// # Example
    ///
    /// ```ignore
    /// // Reorg detected: blocks 1100-1200 were reorganized
    /// let reorg_info = ReorgInfo {
    ///     detected_at_block: 1250,
    ///     affected_block_range: (1100, 1200),
    ///     removed_log_count: 73,
    /// };
    ///
    /// // Canonical chain resumes at block 1201
    /// let corrected = handle_reorg(&db, &reorg_info, 1201).await?;
    ///
    /// // corrected = number of channels that received corrective states
    /// info!(corrected, "Reorg handling complete");
    /// ```
    ///
    /// # See Also
    ///
    /// - [`ReorgInfo`] - Structure containing reorg detection information
    /// - [`get_channel_state_at`](blokli_db::state_queries::get_channel_state_at) - Temporal queries that work with
    ///   corrective states
    /// - Design document section 6.5 - Detailed reorg handling specification
    async fn handle_reorg(
        db: &Db,
        reorg_info: &ReorgInfo,
        canonical_block: u64,
        indexer_state: &IndexerState,
    ) -> Result<usize>
    where
        Db: BlokliDbGeneralModelOperations
            + BlokliDbInfoOperations
            + BlokliDbLogOperations
            + Clone
            + Send
            + Sync
            + 'static,
    {
        let (min_block, max_block) = reorg_info.affected_block_range;
        let db_conn = db.conn(TargetDb::Index);

        info!(
            min_block,
            max_block, canonical_block, "Processing reorg: identifying affected channels"
        );

        // Step 1: Query channel_state table for all states in the affected block range
        // This tells us which channels were affected by the reorg
        let affected_states = ChannelState::find()
            .filter(
                Condition::all()
                    .add(channel_state::Column::PublishedBlock.gte(min_block))
                    .add(channel_state::Column::PublishedBlock.lte(max_block)),
            )
            .all(db_conn)
            .await
            .map_err(|e| {
                CoreEthereumIndexerError::ProcessError(format!("Failed to query affected channel states: {}", e))
            })?;

        // Extract unique channel IDs
        let affected_channel_ids: HashSet<i64> = affected_states.iter().map(|state| state.channel_id).collect();

        info!(affected_count = affected_channel_ids.len(), "Found affected channels");

        let mut corrected_count = 0;

        // Step 2: For each affected channel, find the last valid state before the reorg
        for channel_id in affected_channel_ids {
            // Query for the last state before the minimum affected block
            let last_valid_state = ChannelState::find()
                .filter(channel_state::Column::ChannelId.eq(channel_id))
                .filter(channel_state::Column::PublishedBlock.lt(min_block))
                .order_by_desc(channel_state::Column::PublishedBlock)
                .order_by_desc(channel_state::Column::PublishedTxIndex)
                .order_by_desc(channel_state::Column::PublishedLogIndex)
                .one(db_conn)
                .await
                .map_err(|e| {
                    CoreEthereumIndexerError::ProcessError(format!(
                        "Failed to query last valid state for channel {}: {}",
                        channel_id, e
                    ))
                })?;

            // If no prior state exists, the channel was opened during the reorg
            // In this case, no correction is needed - the channel will be re-indexed
            let Some(valid_state) = last_valid_state else {
                debug!(channel_id, "Channel was opened during reorg, skipping correction");
                continue;
            };

            // Step 3: Insert corrective state at synthetic position (canonical_block, 0, 0)
            // This state represents the canonical state after reorg recovery
            let corrective_state = channel_state::ActiveModel {
                id: Default::default(), // Auto-increment
                channel_id: Set(channel_id),
                balance: Set(valid_state.balance),
                status: Set(valid_state.status),
                epoch: Set(valid_state.epoch),
                ticket_index: Set(valid_state.ticket_index),
                closure_time: Set(valid_state.closure_time),
                corrupted_state: Set(valid_state.corrupted_state),
                published_block: Set(canonical_block as i64),
                published_tx_index: Set(0),  // Synthetic position
                published_log_index: Set(0), // Synthetic position
                reorg_correction: Set(true), // Mark as reorg correction
            };

            corrective_state.insert(db_conn).await.map_err(|e| {
                CoreEthereumIndexerError::ProcessError(format!(
                    "Failed to insert corrective state for channel {}: {}",
                    channel_id, e
                ))
            })?;

            corrected_count += 1;

            debug!(channel_id, canonical_block, "Inserted corrective state");
        }

        // Signal shutdown to active subscriptions
        // Subscriptions will detect shutdown signal and close client connections,
        // forcing clients to reconnect and get fresh watermarks
        if !indexer_state.signal_shutdown() {
            error!("Failed to signal shutdown to subscriptions after reorg - channel may be closed");
        } else {
            info!("Signaled shutdown to active subscriptions after reorg");
        }

        info!(corrected_count, "Reorg handling complete");

        Ok(corrected_count)
    }

    async fn update_chain_head(rpc: &T, chain_head: Arc<AtomicU64>) -> u64
    where
        T: HoprIndexerRpcOperations + 'static,
    {
        match rpc.block_number().await {
            Ok(head) => {
                chain_head.store(head, Ordering::Relaxed);
                debug!(head, "Updated chain head");
                head
            }
            Err(error) => {
                error!(%error, "Failed to fetch block number from RPC");
                panic!("Failed to fetch block number from RPC, cannot continue indexing due to {error}")
            }
        }
    }

    /// Calculates the synchronization progress.
    ///
    /// This function processes a block and updates synchronization metrics and state.
    ///
    /// # Arguments
    ///
    /// * `block` - The block with logs to process.
    /// * `rpc` - The RPC operations handler.
    /// * `chain_head` - The current chain head block number.
    /// * `is_synced` - A boolean indicating whether the indexer is synced.
    /// * `start_block` - The first block number to process.
    /// * `tx` - A sender channel for synchronization notifications.
    ///
    /// # Returns
    ///
    /// The block which was provided as input.
    #[allow(clippy::too_many_arguments)]
    async fn calculate_sync_process(
        current_block: u64,
        rpc: &T,
        _db: Db,
        chain_head: Arc<AtomicU64>,
        is_synced: Arc<AtomicBool>,
        next_block_to_process: u64,
        mut tx: futures::channel::mpsc::Sender<()>,
        _channels_address: Option<Address>,
    ) where
        T: HoprIndexerRpcOperations + 'static,
        Db: BlokliDbInfoOperations + Clone + Send + Sync + 'static,
    {
        #[cfg(all(feature = "prometheus", not(test)))]
        {
            METRIC_INDEXER_CURRENT_BLOCK.set(current_block as f64);
        }

        let mut head = chain_head.load(Ordering::Relaxed);

        // We only print out sync progress if we are not yet synced.
        // Once synced, we don't print out progress anymore.
        if !is_synced.load(Ordering::Relaxed) {
            let mut block_difference = head.saturating_sub(next_block_to_process);

            let progress = if block_difference == 0 {
                // Before we call the sync complete, we check the chain again.
                head = Self::update_chain_head(rpc, chain_head.clone()).await;
                block_difference = head.saturating_sub(next_block_to_process);

                if block_difference == 0 {
                    1_f64
                } else {
                    (current_block - next_block_to_process) as f64 / block_difference as f64
                }
            } else {
                (current_block - next_block_to_process) as f64 / block_difference as f64
            };

            info!(
                progress = progress * 100_f64,
                block = current_block,
                head,
                "Sync progress to last known head"
            );

            #[cfg(all(feature = "prometheus", not(test)))]
            METRIC_INDEXER_SYNC_PROGRESS.set(progress);

            if current_block >= head {
                info!("indexer sync completed successfully");
                is_synced.store(true, Ordering::Relaxed);

                if let Err(error) = tx.try_send(()) {
                    error!(%error, "failed to notify about achieving indexer synchronization")
                }
            }
        }
    }

    /// Checks if the logs database has any existing data.
    ///
    /// This method determines whether the database already contains logs, which helps
    /// decide whether to download a snapshot for faster synchronization. It queries
    /// the database for the total log count and returns an error if the query fails
    /// (e.g., when the database doesn't exist yet).
    ///
    /// # Returns
    ///
    /// - `Ok(true)` if the database contains one or more logs
    /// - `Ok(false)` if the database is empty
    /// - `Err(e)` if the database cannot be queried
    async fn has_logs_data(&self) -> Result<bool> {
        self.db
            .get_logs_count(None, None)
            .await
            .map(|count| count > 0)
            .map_err(|e| CoreEthereumIndexerError::SnapshotError(e.to_string()))
    }

    /// Downloads and installs a database snapshot for faster initial synchronization.
    ///
    /// This method coordinates the snapshot download process by:
    /// 1. Validating the indexer configuration
    /// 2. Creating a snapshot manager instance
    /// 3. Downloading and extracting the snapshot to the data directory
    ///
    /// Snapshots allow new nodes to quickly synchronize with the network by downloading
    /// pre-built database files instead of fetching all historical logs from scratch.
    ///
    /// # Returns
    ///
    /// - `Ok(SnapshotInfo)` containing details about the downloaded snapshot
    /// - `Err(CoreEthereumIndexerError::SnapshotError)` if validation or download fails
    ///
    /// # Prerequisites
    ///
    /// - Configuration must be valid (proper URL format, data directory set)
    /// - Sufficient disk space must be available
    /// - Network connectivity to the snapshot URL
    pub async fn download_snapshot(&self) -> Result<SnapshotInfo> {
        // Validate config before proceeding
        if let Err(e) = self.cfg.validate() {
            return Err(CoreEthereumIndexerError::SnapshotError(e.to_string()));
        }

        let snapshot_manager = SnapshotManager::with_db(self.db.clone())
            .map_err(|e| CoreEthereumIndexerError::SnapshotError(e.to_string()))?;

        let data_dir = Path::new(&self.cfg.data_directory);

        // The URL has been verified so we can just use it.
        if let Some(url) = &self.cfg.logs_snapshot_url {
            snapshot_manager
                .download_and_setup_snapshot(url, data_dir)
                .await
                .map_err(|e| CoreEthereumIndexerError::SnapshotError(e.to_string()))
        } else {
            Err(CoreEthereumIndexerError::SnapshotError(
                "Logs snapshot URL is not configured".to_string(),
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::BTreeSet, pin::Pin};

    use alloy::{
        dyn_abi::DynSolValue,
        primitives::{Address as AlloyAddress, B256},
    };
    use async_trait::async_trait;
    use blokli_chain_rpc::BlockWithLogs;
    use blokli_chain_types::{ContractAddresses, chain_events::ChainEventType};
    use blokli_db::{
        TargetDb, accounts::BlokliDbAccountOperations, db::BlokliDb, events::BlockPosition,
        state_queries::get_channel_state_at,
    };
    use blokli_db_entity::{
        account, channel,
        prelude::{Account, Channel},
    };
    use futures::{Stream, join};
    use hex_literal::hex;
    use hopr_crypto_types::{
        keypairs::{Keypair, OffchainKeypair},
        prelude::ChainKeypair,
    };
    use hopr_internal_types::account::{AccountEntry, AccountType};
    use hopr_primitive_types::prelude::*;
    use mockall::mock;
    use multiaddr::Multiaddr;

    use super::*;
    use crate::traits::MockChainLogHandler;

    lazy_static::lazy_static! {
        static ref ALICE_OKP: OffchainKeypair = OffchainKeypair::random();
        static ref ALICE_KP: ChainKeypair = ChainKeypair::from_secret(&hex!("492057cf93e99b31d2a85bc5e98a9c3aa0021feec52c227cc8170e8f7d047775")).expect("lazy static keypair should be constructible");
        static ref ALICE: Address = ALICE_KP.public().to_address();
        static ref BOB_OKP: OffchainKeypair = OffchainKeypair::random();
        static ref BOB: Address = hex!("3798fa65d6326d3813a0d33489ac35377f4496ef").into();
        static ref CHRIS: Address = hex!("250eefb2586ab0873befe90b905126810960ee7c").into();

        static ref RANDOM_ANNOUNCEMENT_CHAIN_EVENT: ChainEventType = ChainEventType::Announcement {
            peer: (*OffchainKeypair::from_secret(&hex!("14d2d952715a51aadbd4cc6bfac9aa9927182040da7b336d37d5bb7247aa7566")).expect("lazy static keypair should be constructible").public()).into(),
            address: hex!("2f4b7662a192b8125bbf51cfbf1bf5cc00b2c8e5").into(),
            multiaddresses: vec![Multiaddr::empty()],
        };
    }

    fn build_announcement_logs(
        address: Address,
        size: usize,
        block_number: u64,
        starting_log_index: u64,
    ) -> anyhow::Result<Vec<SerializableLog>> {
        let mut logs: Vec<SerializableLog> = vec![];
        let block_hash = Hash::create(&[format!("my block hash {block_number}").as_bytes()]);

        for i in 0..size {
            let test_multiaddr: Multiaddr = format!("/ip4/1.2.3.4/tcp/{}", 1000 + i).parse()?;
            let tx_index: u64 = i as u64;
            let log_index: u64 = starting_log_index + tx_index;

            logs.push(SerializableLog {
                address,
                block_hash: block_hash.into(),
                topics: vec![hopr_bindings::hopr_announcements_events::HoprAnnouncementsEvents::AddressAnnouncement::SIGNATURE_HASH.into()],
                data: DynSolValue::Tuple(vec![
                    DynSolValue::Address(AlloyAddress::from_hopr_address(address)),
                    DynSolValue::String(test_multiaddr.to_string()),
                ])
                .abi_encode(),
                tx_hash: Hash::create(&[format!("my tx hash {i}").as_bytes()]).into(),
                tx_index,
                block_number,
                log_index,
                ..Default::default()
            });
        }

        Ok(logs)
    }

    mock! {
        HoprIndexerOps {}     // Name of the mock struct, less the "Mock" prefix

        #[async_trait]
        impl HoprIndexerRpcOperations for HoprIndexerOps {
            async fn block_number(&self) -> blokli_chain_rpc::errors::Result<u64>;
            async fn get_hopr_allowance(&self, owner: Address, spender: Address) -> blokli_chain_rpc::errors::Result<HoprBalance>;
            async fn get_xdai_balance(&self, address: Address) -> blokli_chain_rpc::errors::Result<XDaiBalance>;
            async fn get_hopr_balance(&self, address: Address) -> blokli_chain_rpc::errors::Result<HoprBalance>;
            async fn get_safe_transaction_count(&self, safe_address: Address) -> blokli_chain_rpc::errors::Result<u64>;

            fn try_stream_logs<'a>(
                &'a self,
                start_block_number: u64,
                filters: FilterSet,
                is_synced: bool,
            ) -> blokli_chain_rpc::errors::Result<Pin<Box<dyn Stream<Item = BlockWithLogs> + Send + 'a>>>;
        }
    }

    #[tokio::test]
    async fn test_indexer_should_check_the_db_for_last_processed_block_and_supply_none_if_none_is_found()
    -> anyhow::Result<()> {
        let mut handlers = MockChainLogHandler::new();
        let mut rpc = MockHoprIndexerOps::new();
        let db = BlokliDb::new_in_memory().await?;

        let addr = Address::new(b"my address 123456789");
        let topic = Hash::create(&[b"my topic"]);
        db.ensure_logs_origin(vec![(addr, topic)]).await?;

        handlers.expect_contract_addresses().return_const(vec![addr]);
        handlers
            .expect_contract_address_topics()
            .withf(move |x| x == &addr)
            .return_const(vec![B256::from_slice(topic.as_ref())]);
        handlers
            .expect_contract_address_topics()
            .withf(move |x| x == &addr)
            .return_const(vec![B256::from_slice(Hash::create(&[b"my topic"]).as_ref())]);
        handlers
            .expect_contract_addresses_map()
            .return_const(ContractAddresses::default());

        let head_block = 1000;
        rpc.expect_block_number().times(2).returning(move || Ok(head_block));

        let (tx, rx) = futures::channel::mpsc::unbounded::<BlockWithLogs>();
        rpc.expect_try_stream_logs()
            .withf(move |x: &u64, _y: &FilterSet, _: &bool| *x == 0)
            .return_once(move |_, _, _| Ok(Box::pin(rx)));

        let indexer = Indexer::new(
            rpc,
            handlers,
            db.clone(),
            IndexerConfig::default(),
            IndexerState::default(),
        )
        .without_panic_on_completion();

        let (indexing, _) = join!(indexer.start(), async move {
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
            tx.close_channel()
        });
        assert!(indexing.is_err()); // terminated by the close channel

        Ok(())
    }

    #[tokio::test]
    async fn test_indexer_should_check_the_db_for_last_processed_block_and_supply_it_when_found() -> anyhow::Result<()>
    {
        let mut handlers = MockChainLogHandler::new();
        let mut rpc = MockHoprIndexerOps::new();
        let db = BlokliDb::new_in_memory().await?;
        let head_block = 1000;
        let latest_block = 15u64;

        let addr = Address::new(b"my address 123456789");
        let topic = Hash::create(&[b"my topic"]);

        handlers.expect_contract_addresses().return_const(vec![addr]);
        handlers
            .expect_contract_address_topics()
            .withf(move |x| x == &addr)
            .return_const(vec![B256::from_slice(topic.as_ref())]);
        handlers
            .expect_contract_address_topics()
            .withf(move |x| x == &addr)
            .return_const(vec![B256::from_slice(Hash::create(&[b"my topic"]).as_ref())]);
        handlers
            .expect_contract_addresses_map()
            .return_const(ContractAddresses::default());

        db.ensure_logs_origin(vec![(addr, topic)]).await?;

        rpc.expect_block_number().times(2).returning(move || Ok(head_block));

        let (tx, rx) = futures::channel::mpsc::unbounded::<BlockWithLogs>();
        rpc.expect_try_stream_logs()
            .once()
            .withf(move |x: &u64, _y: &FilterSet, _: &bool| *x == latest_block + 1)
            .return_once(move |_, _, _| Ok(Box::pin(rx)));

        // insert and process latest block
        let log_1 = SerializableLog {
            address: Address::new(b"my address 123456789"),
            topics: [Hash::create(&[b"my topic"]).into()].into(),
            data: [1, 2, 3, 4].into(),
            tx_index: 1u64,
            block_number: latest_block,
            block_hash: Hash::create(&[b"my block hash"]).into(),
            tx_hash: Hash::create(&[b"my tx hash"]).into(),
            log_index: 1u64,
            removed: false,
            processed: Some(false),
            ..Default::default()
        };
        assert!(db.store_log(log_1.clone()).await.is_ok());
        assert!(db.set_logs_processed(Some(latest_block), Some(0)).await.is_ok());
        assert!(db.update_logs_checksums().await.is_ok());

        let indexer = Indexer::new(
            rpc,
            handlers,
            db.clone(),
            IndexerConfig {
                fast_sync: false,
                ..Default::default()
            },
            IndexerState::default(),
        )
        .without_panic_on_completion();

        let (indexing, _) = join!(indexer.start(), async move {
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
            tx.close_channel()
        });
        assert!(indexing.is_err()); // terminated by the close channel

        Ok(())
    }

    #[tokio::test]
    async fn test_indexer_should_pass_blocks_that_are_finalized() -> anyhow::Result<()> {
        let mut handlers = MockChainLogHandler::new();
        let mut rpc = MockHoprIndexerOps::new();
        let db = BlokliDb::new_in_memory().await?;

        let cfg = IndexerConfig::default();

        let addr = Address::new(b"my address 123456789");
        handlers.expect_contract_addresses().return_const(vec![addr]);
        handlers
            .expect_contract_address_topics()
            .withf(move |x| x == &addr)
            .return_const(vec![B256::from_slice(Hash::create(&[b"my topic"]).as_ref())]);
        handlers
            .expect_contract_addresses_map()
            .return_const(ContractAddresses::default());

        let (mut tx, rx) = futures::channel::mpsc::unbounded::<BlockWithLogs>();
        rpc.expect_try_stream_logs()
            .times(1)
            .withf(move |x: &u64, _y: &FilterSet, _: &bool| *x == 0)
            .return_once(move |_, _, _| Ok(Box::pin(rx)));

        let head_block = 1000;
        rpc.expect_block_number().returning(move || Ok(head_block));

        rpc.expect_get_hopr_balance()
            .withf(move |x| x == &Address::new(b"my safe address 1234"))
            .returning(move |_| Ok(HoprBalance::default()));

        rpc.expect_get_hopr_allowance()
            .withf(move |x, y| x == &Address::new(b"my safe address 1234") && y == &Address::from([0; 20]))
            .returning(move |_, _| Ok(HoprBalance::default()));

        let finalized_block = BlockWithLogs {
            block_id: head_block - 1,
            logs: BTreeSet::from_iter(build_announcement_logs(*BOB, 4, head_block - 1, 23)?),
        };
        let head_allowing_finalization = BlockWithLogs {
            block_id: head_block,
            logs: BTreeSet::new(),
        };

        // called once per block which is finalizable
        handlers
            .expect_collect_log_event()
            // .times(2)
            .times(finalized_block.logs.len())
            .returning(|_, _| Ok(()));

        assert!(tx.start_send(finalized_block.clone()).is_ok());
        assert!(tx.start_send(head_allowing_finalization.clone()).is_ok());

        let indexer =
            Indexer::new(rpc, handlers, db.clone(), cfg, IndexerState::default()).without_panic_on_completion();
        let _ = join!(indexer.start(), async move {
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
            tx.close_channel()
        });

        Ok(())
    }

    #[test_log::test(tokio::test)]
    async fn test_indexer_fast_sync_full_with_resume() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;

        let addr = Address::new(b"my address 123456789");
        let topic = Hash::create(&[b"my topic"]);

        // Run 1: Fast sync enabled, index empty
        {
            let logs = vec![
                build_announcement_logs(*BOB, 1, 1, 1)?,
                build_announcement_logs(*BOB, 1, 2, 1)?,
            ]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>();

            assert!(db.ensure_logs_origin(vec![(addr, topic)]).await.is_ok());

            for log in logs {
                assert!(db.store_log(log).await.is_ok());
            }
            assert!(db.update_logs_checksums().await.is_ok());
            assert_eq!(db.get_logs_block_numbers(None, None, Some(true)).await?.len(), 0);
            assert_eq!(db.get_logs_block_numbers(None, None, Some(false)).await?.len(), 2);

            let (tx, rx) = futures::channel::mpsc::unbounded::<BlockWithLogs>();

            let head_block = 5;
            let mut rpc = MockHoprIndexerOps::new();
            rpc.expect_block_number().returning(move || Ok(head_block));
            rpc.expect_try_stream_logs()
                .times(1)
                .withf(move |x: &u64, _y: &FilterSet, _: &bool| *x == 3)
                .return_once(move |_, _, _| Ok(Box::pin(rx)));

            let mut handlers = MockChainLogHandler::new();
            handlers.expect_contract_addresses().return_const(vec![addr]);
            handlers
                .expect_contract_address_topics()
                .withf(move |x| x == &addr)
                .return_const(vec![B256::from_slice(topic.as_ref())]);
            handlers
                .expect_collect_log_event()
                .times(2)
                .withf(move |l, _| [1, 2].contains(&l.block_number))
                .returning(|_, _| Ok(()));
            handlers
                .expect_contract_address_topics()
                .withf(move |x| x == &addr)
                .return_const(vec![B256::from_slice(Hash::create(&[b"my topic"]).as_ref())]);
            handlers
                .expect_contract_addresses_map()
                .return_const(ContractAddresses::default());

            let indexer_cfg = IndexerConfig::new(0, true, false, None, "/tmp/test_data".to_string(), 1000, 10);
            let indexer = Indexer::new(rpc, handlers, db.clone(), indexer_cfg, IndexerState::default())
                .without_panic_on_completion();
            let (indexing, _) = join!(indexer.start(), async move {
                tokio::time::sleep(std::time::Duration::from_millis(200)).await;
                tx.close_channel()
            });
            assert!(indexing.is_err()); // terminated by the close channel

            assert_eq!(db.get_logs_block_numbers(None, None, Some(true)).await?.len(), 2);
            assert_eq!(db.get_logs_block_numbers(None, None, Some(false)).await?.len(), 0);

            // At the end we need to simulate that the index is not empty,
            // thus storing some data.
            db.insert_account(
                None,
                AccountEntry {
                    public_key: *ALICE_OKP.public(),
                    chain_addr: *ALICE,
                    entry_type: AccountType::NotAnnounced,
                    safe_address: None,
                    key_id: 0.into(),
                },
            )
            .await?;
            db.insert_account(
                None,
                AccountEntry {
                    public_key: *BOB_OKP.public(),
                    chain_addr: *BOB,
                    entry_type: AccountType::NotAnnounced,
                    safe_address: None,
                    key_id: 0.into(),
                },
            )
            .await?;
        }

        // Run 2: Fast sync enabled, index not empty, resume after 2 logs
        {
            let logs = vec![
                build_announcement_logs(*BOB, 1, 3, 1)?,
                build_announcement_logs(*BOB, 1, 4, 1)?,
            ]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>();

            assert!(db.ensure_logs_origin(vec![(addr, topic)]).await.is_ok());

            for log in logs {
                assert!(db.store_log(log).await.is_ok());
            }
            assert!(db.update_logs_checksums().await.is_ok());
            assert_eq!(db.get_logs_block_numbers(None, None, Some(true)).await?.len(), 2);
            assert_eq!(db.get_logs_block_numbers(None, None, Some(false)).await?.len(), 2);

            let (tx, rx) = futures::channel::mpsc::unbounded::<BlockWithLogs>();

            let head_block = 5;
            let mut rpc = MockHoprIndexerOps::new();
            rpc.expect_block_number().returning(move || Ok(head_block));
            rpc.expect_try_stream_logs()
                .times(1)
                .withf(move |x: &u64, _y: &FilterSet, _: &bool| *x == 5)
                .return_once(move |_, _, _| Ok(Box::pin(rx)));

            let mut handlers = MockChainLogHandler::new();
            handlers.expect_contract_addresses().return_const(vec![addr]);
            handlers
                .expect_contract_address_topics()
                .withf(move |x| x == &addr)
                .return_const(vec![B256::from_slice(topic.as_ref())]);

            handlers
                .expect_collect_log_event()
                .times(2)
                .withf(move |l, _| [3, 4].contains(&l.block_number))
                .returning(|_, _| Ok(()));
            handlers
                .expect_contract_address_topics()
                .withf(move |x| x == &addr)
                .return_const(vec![B256::from_slice(Hash::create(&[b"my topic"]).as_ref())]);
            handlers
                .expect_contract_addresses_map()
                .return_const(ContractAddresses::default());

            let indexer_cfg = IndexerConfig::new(0, true, false, None, "/tmp/test_data".to_string(), 1000, 10);
            let indexer = Indexer::new(rpc, handlers, db.clone(), indexer_cfg, IndexerState::default())
                .without_panic_on_completion();
            let (indexing, _) = join!(indexer.start(), async move {
                tokio::time::sleep(std::time::Duration::from_millis(200)).await;
                tx.close_channel()
            });
            assert!(indexing.is_err()); // terminated by the close channel

            assert_eq!(db.get_logs_block_numbers(None, None, Some(true)).await?.len(), 4);
            assert_eq!(db.get_logs_block_numbers(None, None, Some(false)).await?.len(), 0);
        }

        Ok(())
    }

    #[test_log::test(tokio::test)]
    async fn test_indexer_should_yield_back_once_the_past_events_are_indexed() -> anyhow::Result<()> {
        let mut handlers = MockChainLogHandler::new();
        let mut rpc = MockHoprIndexerOps::new();
        let db = BlokliDb::new_in_memory().await?;

        let cfg = IndexerConfig::default();

        // We don't want to index anything really
        let addr = Address::new(b"my address 123456789");
        handlers.expect_contract_addresses().return_const(vec![addr]);
        handlers
            .expect_contract_address_topics()
            .withf(move |x| x == &addr)
            .return_const(vec![B256::from_slice(Hash::create(&[b"my topic"]).as_ref())]);
        handlers
            .expect_contract_address_topics()
            .withf(move |x| x == &addr)
            .return_const(vec![B256::from_slice(Hash::create(&[b"my topic"]).as_ref())]);
        handlers
            .expect_contract_addresses_map()
            .return_const(ContractAddresses::default());

        let (mut tx, rx) = futures::channel::mpsc::unbounded::<BlockWithLogs>();
        // Expected to be called once starting at 0 and yield the respective blocks
        rpc.expect_try_stream_logs()
            .times(1)
            .withf(move |x: &u64, _y: &FilterSet, _: &bool| *x == 0)
            .return_once(move |_, _, _| Ok(Box::pin(rx)));
        rpc.expect_get_hopr_balance()
            .once()
            .return_once(move |_| Ok(HoprBalance::zero()));
        rpc.expect_get_hopr_allowance()
            .once()
            .return_once(move |_, _| Ok(HoprBalance::zero()));

        let head_block = 1000;
        let block_numbers = [head_block - 1, head_block, head_block + 1];

        let blocks: Vec<BlockWithLogs> = block_numbers
            .iter()
            .map(|block_id| BlockWithLogs {
                block_id: *block_id,
                logs: BTreeSet::from_iter(build_announcement_logs(*ALICE, 1, *block_id, 23).unwrap()),
            })
            .collect();

        for _ in 0..(blocks.len() as u64) {
            rpc.expect_block_number().returning(move || Ok(head_block));
        }

        for block in blocks.iter() {
            assert!(tx.start_send(block.clone()).is_ok());
        }

        // Generate the expected events to be able to process the blocks
        handlers
            .expect_collect_log_event()
            .times(1)
            .withf(move |l, _| block_numbers.contains(&l.block_number))
            .returning(|_, _| Ok(()));

        let indexer =
            Indexer::new(rpc, handlers, db.clone(), cfg, IndexerState::default()).without_panic_on_completion();
        indexer.start().await?;

        // Test validates that indexer processes blocks correctly once past events are indexed
        // Event publishing behavior is now handled separately through IndexerState

        Ok(())
    }

    #[test_log::test(tokio::test)]
    async fn test_indexer_should_not_reprocess_last_processed_block() -> anyhow::Result<()> {
        let last_processed_block = 100_u64;

        let db = BlokliDb::new_in_memory().await?;

        let addr = Address::new(b"my address 123456789");
        let topic = Hash::create(&[b"my topic"]);
        assert!(db.ensure_logs_origin(vec![(addr, topic)]).await.is_ok());

        // insert and process latest block
        let log_1 = SerializableLog {
            address: Address::new(b"my address 123456789"),
            topics: [Hash::create(&[b"my topic"]).into()].into(),
            data: [1, 2, 3, 4].into(),
            tx_index: 1u64,
            block_number: last_processed_block,
            block_hash: Hash::create(&[b"my block hash"]).into(),
            tx_hash: Hash::create(&[b"my tx hash"]).into(),
            log_index: 1u64,
            removed: false,
            processed: Some(false),
            ..Default::default()
        };
        assert!(db.store_log(log_1.clone()).await.is_ok());
        assert!(db.set_logs_processed(Some(last_processed_block), Some(0)).await.is_ok());
        assert!(db.update_logs_checksums().await.is_ok());

        let (mut tx, rx) = futures::channel::mpsc::unbounded::<BlockWithLogs>();

        let mut rpc = MockHoprIndexerOps::new();
        rpc.expect_try_stream_logs()
            .once()
            .withf(move |x: &u64, _y: &FilterSet, _: &bool| *x == last_processed_block + 1)
            .return_once(move |_, _, _| Ok(Box::pin(rx)));

        rpc.expect_block_number()
            .times(3)
            .returning(move || Ok(last_processed_block + 1));

        rpc.expect_get_hopr_balance()
            .once()
            .return_once(move |_| Ok(HoprBalance::zero()));

        rpc.expect_get_hopr_allowance()
            .once()
            .return_once(move |_, _| Ok(HoprBalance::zero()));

        let block = BlockWithLogs {
            block_id: last_processed_block + 1,
            logs: BTreeSet::from_iter(build_announcement_logs(*ALICE, 1, last_processed_block + 1, 23)?),
        };

        tx.start_send(block)?;

        let mut handlers = MockChainLogHandler::new();
        handlers.expect_contract_addresses().return_const(vec![addr]);
        handlers
            .expect_contract_address_topics()
            .withf(move |x| x == &addr)
            .return_const(vec![B256::from_slice(topic.as_ref())]);
        handlers
            .expect_contract_address_topics()
            .withf(move |x| x == &addr)
            .return_const(vec![B256::from_slice(Hash::create(&[b"my topic"]).as_ref())]);
        handlers
            .expect_contract_addresses_map()
            .return_const(ContractAddresses::default());

        let indexer_cfg = IndexerConfig::new(0, false, false, None, "/tmp/test_data".to_string(), 1000, 10);

        let indexer =
            Indexer::new(rpc, handlers, db.clone(), indexer_cfg, IndexerState::default()).without_panic_on_completion();
        indexer.start().await?;

        Ok(())
    }

    // ==================== Reorg Detection Tests ====================

    #[test]
    fn test_detect_reorg_returns_none_when_no_removed_logs() {
        // Test that blocks with all logs having removed=false return None
        let block = BlockWithLogs {
            block_id: 100,
            logs: BTreeSet::from([
                SerializableLog {
                    address: Address::new(b"test address 1234567"),
                    topics: vec![Hash::create(&[b"topic"]).into()],
                    data: vec![1, 2, 3],
                    tx_index: 0,
                    block_number: 100,
                    block_hash: Hash::create(&[b"block 100"]).into(),
                    tx_hash: Hash::create(&[b"tx 0"]).into(),
                    log_index: 0,
                    removed: false,
                    processed: None,
                    checksum: None,
                    processed_at: None,
                },
                SerializableLog {
                    address: Address::new(b"test address 1234567"),
                    topics: vec![Hash::create(&[b"topic"]).into()],
                    data: vec![4, 5, 6],
                    tx_index: 1,
                    block_number: 100,
                    block_hash: Hash::create(&[b"block 100"]).into(),
                    tx_hash: Hash::create(&[b"tx 1"]).into(),
                    log_index: 1,
                    removed: false,
                    processed: None,
                    checksum: None,
                    processed_at: None,
                },
            ]),
        };

        let result = Indexer::<MockHoprIndexerOps, MockChainLogHandler, BlokliDb>::detect_reorg(&block);
        assert!(result.is_none(), "Expected None when no logs are marked as removed");
    }

    #[test]
    fn test_detect_reorg_detects_single_removed_log() {
        // Test that a single removed log is properly detected
        let block = BlockWithLogs {
            block_id: 200,
            logs: BTreeSet::from([
                SerializableLog {
                    address: Address::new(b"test address 1234567"),
                    topics: vec![Hash::create(&[b"topic"]).into()],
                    data: vec![1, 2, 3],
                    tx_index: 0,
                    block_number: 199,
                    block_hash: Hash::create(&[b"block 199"]).into(),
                    tx_hash: Hash::create(&[b"tx 0"]).into(),
                    log_index: 0,
                    removed: true, // This log was reorged
                    processed: None,
                    checksum: None,
                    processed_at: None,
                },
                SerializableLog {
                    address: Address::new(b"test address 1234567"),
                    topics: vec![Hash::create(&[b"topic"]).into()],
                    data: vec![4, 5, 6],
                    tx_index: 1,
                    block_number: 200,
                    block_hash: Hash::create(&[b"block 200"]).into(),
                    tx_hash: Hash::create(&[b"tx 1"]).into(),
                    log_index: 1,
                    removed: false,
                    processed: None,
                    checksum: None,
                    processed_at: None,
                },
            ]),
        };

        let result = Indexer::<MockHoprIndexerOps, MockChainLogHandler, BlokliDb>::detect_reorg(&block);
        assert!(result.is_some(), "Expected Some when removed logs are present");

        let reorg_info = result.unwrap();
        assert_eq!(reorg_info.detected_at_block, 200);
        assert_eq!(reorg_info.affected_block_range, (199, 199));
        assert_eq!(reorg_info.removed_log_count, 1);
    }

    #[test]
    fn test_detect_reorg_detects_multiple_removed_logs_same_block() {
        // Test multiple removed logs from the same block
        let block = BlockWithLogs {
            block_id: 150,
            logs: BTreeSet::from([
                SerializableLog {
                    address: Address::new(b"test address 1234567"),
                    topics: vec![Hash::create(&[b"topic"]).into()],
                    data: vec![1, 2, 3],
                    tx_index: 0,
                    block_number: 148,
                    block_hash: Hash::create(&[b"block 148"]).into(),
                    tx_hash: Hash::create(&[b"tx 0"]).into(),
                    log_index: 0,
                    removed: true,
                    processed: None,
                    checksum: None,
                    processed_at: None,
                },
                SerializableLog {
                    address: Address::new(b"test address 1234567"),
                    topics: vec![Hash::create(&[b"topic"]).into()],
                    data: vec![4, 5, 6],
                    tx_index: 1,
                    block_number: 148,
                    block_hash: Hash::create(&[b"block 148"]).into(),
                    tx_hash: Hash::create(&[b"tx 1"]).into(),
                    log_index: 1,
                    removed: true,
                    processed: None,
                    checksum: None,
                    processed_at: None,
                },
                SerializableLog {
                    address: Address::new(b"test address 1234567"),
                    topics: vec![Hash::create(&[b"topic"]).into()],
                    data: vec![7, 8, 9],
                    tx_index: 2,
                    block_number: 150,
                    block_hash: Hash::create(&[b"block 150"]).into(),
                    tx_hash: Hash::create(&[b"tx 2"]).into(),
                    log_index: 2,
                    removed: false,
                    processed: None,
                    checksum: None,
                    processed_at: None,
                },
            ]),
        };

        let result = Indexer::<MockHoprIndexerOps, MockChainLogHandler, BlokliDb>::detect_reorg(&block);
        assert!(result.is_some(), "Expected Some when removed logs are present");

        let reorg_info = result.unwrap();
        assert_eq!(reorg_info.detected_at_block, 150);
        assert_eq!(reorg_info.affected_block_range, (148, 148));
        assert_eq!(reorg_info.removed_log_count, 2);
    }

    #[test]
    fn test_detect_reorg_detects_multiple_removed_logs_different_blocks() {
        // Test removed logs from different blocks - calculates correct range
        let block = BlockWithLogs {
            block_id: 300,
            logs: BTreeSet::from([
                SerializableLog {
                    address: Address::new(b"test address 1234567"),
                    topics: vec![Hash::create(&[b"topic"]).into()],
                    data: vec![1, 2, 3],
                    tx_index: 0,
                    block_number: 295,
                    block_hash: Hash::create(&[b"block 295"]).into(),
                    tx_hash: Hash::create(&[b"tx 0"]).into(),
                    log_index: 0,
                    removed: true,
                    processed: None,
                    checksum: None,
                    processed_at: None,
                },
                SerializableLog {
                    address: Address::new(b"test address 1234567"),
                    topics: vec![Hash::create(&[b"topic"]).into()],
                    data: vec![4, 5, 6],
                    tx_index: 1,
                    block_number: 297,
                    block_hash: Hash::create(&[b"block 297"]).into(),
                    tx_hash: Hash::create(&[b"tx 1"]).into(),
                    log_index: 1,
                    removed: true,
                    processed: None,
                    checksum: None,
                    processed_at: None,
                },
                SerializableLog {
                    address: Address::new(b"test address 1234567"),
                    topics: vec![Hash::create(&[b"topic"]).into()],
                    data: vec![7, 8, 9],
                    tx_index: 2,
                    block_number: 299,
                    block_hash: Hash::create(&[b"block 299"]).into(),
                    tx_hash: Hash::create(&[b"tx 2"]).into(),
                    log_index: 2,
                    removed: true,
                    processed: None,
                    checksum: None,
                    processed_at: None,
                },
                SerializableLog {
                    address: Address::new(b"test address 1234567"),
                    topics: vec![Hash::create(&[b"topic"]).into()],
                    data: vec![10, 11, 12],
                    tx_index: 3,
                    block_number: 300,
                    block_hash: Hash::create(&[b"block 300"]).into(),
                    tx_hash: Hash::create(&[b"tx 3"]).into(),
                    log_index: 3,
                    removed: false,
                    processed: None,
                    checksum: None,
                    processed_at: None,
                },
            ]),
        };

        let result = Indexer::<MockHoprIndexerOps, MockChainLogHandler, BlokliDb>::detect_reorg(&block);
        assert!(result.is_some(), "Expected Some when removed logs are present");

        let reorg_info = result.unwrap();
        assert_eq!(reorg_info.detected_at_block, 300);
        assert_eq!(
            reorg_info.affected_block_range,
            (295, 299),
            "Should span from min (295) to max (299) affected block"
        );
        assert_eq!(reorg_info.removed_log_count, 3);
    }

    #[test]
    fn test_detect_reorg_calculates_correct_affected_range() {
        // Test that min/max calculation works correctly
        let block = BlockWithLogs {
            block_id: 1000,
            logs: BTreeSet::from([
                SerializableLog {
                    address: Address::new(b"test address 1234567"),
                    topics: vec![Hash::create(&[b"topic"]).into()],
                    data: vec![1],
                    tx_index: 0,
                    block_number: 990,
                    block_hash: Hash::create(&[b"block 990"]).into(),
                    tx_hash: Hash::create(&[b"tx 0"]).into(),
                    log_index: 0,
                    removed: true,
                    processed: None,
                    checksum: None,
                    processed_at: None,
                },
                SerializableLog {
                    address: Address::new(b"test address 1234567"),
                    topics: vec![Hash::create(&[b"topic"]).into()],
                    data: vec![2],
                    tx_index: 1,
                    block_number: 998,
                    block_hash: Hash::create(&[b"block 998"]).into(),
                    tx_hash: Hash::create(&[b"tx 1"]).into(),
                    log_index: 1,
                    removed: true,
                    processed: None,
                    checksum: None,
                    processed_at: None,
                },
            ]),
        };

        let result = Indexer::<MockHoprIndexerOps, MockChainLogHandler, BlokliDb>::detect_reorg(&block);
        assert!(result.is_some());

        let reorg_info = result.unwrap();
        assert_eq!(reorg_info.detected_at_block, 1000);
        assert_eq!(reorg_info.affected_block_range, (990, 998));
        assert_eq!(reorg_info.removed_log_count, 2);
    }

    #[test]
    fn test_detect_reorg_empty_block() {
        // Test that an empty block returns None
        let block = BlockWithLogs {
            block_id: 500,
            logs: BTreeSet::new(),
        };

        let result = Indexer::<MockHoprIndexerOps, MockChainLogHandler, BlokliDb>::detect_reorg(&block);
        assert!(result.is_none(), "Expected None for empty block");
    }

    // ==================== Reorg Handling Tests ====================

    // Helper function to create a test channel
    async fn create_test_channel(db: &BlokliDb, channel_id: i64) -> anyhow::Result<()> {
        let conn = db.conn(TargetDb::Index);

        // Check if channel already exists
        let existing = Channel::find_by_id(channel_id).one(conn).await?;
        if existing.is_some() {
            return Ok(());
        }

        // Create source and destination accounts if they don't exist
        let source_id: i64 = channel_id * 10;
        let dest_id: i64 = channel_id * 10 + 1;

        let source_exists = Account::find_by_id(source_id).one(conn).await?.is_some();
        if !source_exists {
            let source_account = account::ActiveModel {
                id: Set(source_id),
                chain_key: Set(vec![1u8; 20]),
                packet_key: Set(format!("source_packet_key_{}", channel_id)),
                published_block: Set(1),
                published_tx_index: Set(0),
                published_log_index: Set(0),
            };
            source_account.insert(conn).await?;
        }

        let dest_exists = Account::find_by_id(dest_id).one(conn).await?.is_some();
        if !dest_exists {
            let dest_account = account::ActiveModel {
                id: Set(dest_id),
                chain_key: Set(vec![3u8; 20]),
                packet_key: Set(format!("dest_packet_key_{}", channel_id)),
                published_block: Set(1),
                published_tx_index: Set(0),
                published_log_index: Set(0),
            };
            dest_account.insert(conn).await?;
        }

        let channel_model = channel::ActiveModel {
            id: Set(channel_id),
            concrete_channel_id: Set(format!("test_channel_{}", channel_id)),
            source: Set(source_id),
            destination: Set(dest_id),
        };
        channel_model.insert(conn).await?;
        Ok(())
    }

    // Helper function to create a channel state
    async fn create_channel_state(
        db: &BlokliDb,
        channel_id: i64,
        block: u64,
        tx_index: u64,
        log_index: u64,
        balance: [u8; 12],
        status: i8,
        reorg_correction: bool,
    ) -> anyhow::Result<()> {
        let conn = db.conn(TargetDb::Index);
        let state = channel_state::ActiveModel {
            id: Default::default(),
            channel_id: Set(channel_id),
            balance: Set(balance.to_vec()),
            status: Set(status),
            epoch: Set(0),
            ticket_index: Set(0),
            closure_time: Set(None),
            corrupted_state: Set(false),
            published_block: Set(block as i64),
            published_tx_index: Set(tx_index as i64),
            published_log_index: Set(log_index as i64),
            reorg_correction: Set(reorg_correction),
        };
        state.insert(conn).await?;
        Ok(())
    }

    // Helper function to get all channel states for a channel
    async fn get_channel_states(
        db: &BlokliDb,
        channel_id: i64,
    ) -> anyhow::Result<Vec<blokli_db_entity::channel_state::Model>> {
        let conn = db.conn(TargetDb::Index);
        let states = ChannelState::find()
            .filter(channel_state::Column::ChannelId.eq(channel_id))
            .order_by_asc(channel_state::Column::PublishedBlock)
            .order_by_asc(channel_state::Column::PublishedTxIndex)
            .order_by_asc(channel_state::Column::PublishedLogIndex)
            .all(conn)
            .await?;
        Ok(states)
    }

    #[tokio::test]
    async fn test_handle_reorg_single_affected_channel() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;

        // Create channel
        create_test_channel(&db, 1).await?;

        // Create channel states: block 10, block 50 (valid), block 100 (will be reorged)
        create_channel_state(&db, 1, 10, 0, 0, [1u8; 12], 1, false).await?;
        create_channel_state(&db, 1, 50, 0, 0, [2u8; 12], 1, false).await?;
        create_channel_state(&db, 1, 100, 0, 0, [3u8; 12], 2, false).await?;

        // Simulate reorg affecting blocks 100-100
        let reorg_info = ReorgInfo {
            detected_at_block: 200,
            affected_block_range: (100, 100),
            removed_log_count: 1,
        };

        // Handle the reorg
        let corrected_count = Indexer::<MockHoprIndexerOps, MockChainLogHandler, BlokliDb>::handle_reorg(
            &db,
            &reorg_info,
            200,
            &IndexerState::default(),
        )
        .await?;

        // Verify correction was made
        assert_eq!(corrected_count, 1, "Expected 1 channel to be corrected");

        // Verify corrective state was inserted
        let states = get_channel_states(&db, 1).await?;
        assert_eq!(states.len(), 4, "Expected 4 states (3 original + 1 corrective)");

        // Find the corrective state
        let corrective_state = states
            .iter()
            .find(|s| s.reorg_correction)
            .expect("Corrective state not found");

        assert_eq!(
            corrective_state.published_block, 200,
            "Corrective state should be at canonical block"
        );
        assert_eq!(
            corrective_state.published_tx_index, 0,
            "Corrective state should have synthetic tx_index"
        );
        assert_eq!(
            corrective_state.published_log_index, 0,
            "Corrective state should have synthetic log_index"
        );
        assert_eq!(
            corrective_state.balance,
            vec![2u8; 12],
            "Corrective state should have balance from last valid state"
        );
        assert_eq!(
            corrective_state.status, 1,
            "Corrective state should have status from last valid state"
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_handle_reorg_multiple_affected_channels() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;

        // Create three channels
        create_test_channel(&db, 1).await?;
        create_test_channel(&db, 2).await?;
        create_test_channel(&db, 3).await?;

        // Channel 1: states at blocks 10, 50, 100 (reorged)
        create_channel_state(&db, 1, 10, 0, 0, [1u8; 12], 1, false).await?;
        create_channel_state(&db, 1, 50, 0, 0, [2u8; 12], 1, false).await?;
        create_channel_state(&db, 1, 100, 0, 0, [3u8; 12], 2, false).await?;

        // Channel 2: states at blocks 20, 90 (reorged)
        create_channel_state(&db, 2, 20, 0, 0, [4u8; 12], 1, false).await?;
        create_channel_state(&db, 2, 90, 0, 0, [5u8; 12], 2, false).await?;

        // Channel 3: states at blocks 30, 95 (reorged), 110 (reorged)
        create_channel_state(&db, 3, 30, 0, 0, [6u8; 12], 1, false).await?;
        create_channel_state(&db, 3, 95, 0, 0, [7u8; 12], 2, false).await?;
        create_channel_state(&db, 3, 110, 0, 0, [8u8; 12], 3, false).await?;

        // Simulate reorg affecting blocks 90-110
        let reorg_info = ReorgInfo {
            detected_at_block: 150,
            affected_block_range: (90, 110),
            removed_log_count: 4,
        };

        // Handle the reorg
        let corrected_count = Indexer::<MockHoprIndexerOps, MockChainLogHandler, BlokliDb>::handle_reorg(
            &db,
            &reorg_info,
            150,
            &IndexerState::default(),
        )
        .await?;

        // Verify all 3 channels were corrected
        assert_eq!(corrected_count, 3, "Expected 3 channels to be corrected");

        // Verify channel 1 correction
        let states_1 = get_channel_states(&db, 1).await?;
        let corrective_1 = states_1
            .iter()
            .find(|s| s.reorg_correction)
            .expect("Channel 1 corrective state not found");
        assert_eq!(
            corrective_1.balance,
            vec![2u8; 12],
            "Channel 1 should rollback to block 50 state"
        );

        // Verify channel 2 correction
        let states_2 = get_channel_states(&db, 2).await?;
        let corrective_2 = states_2
            .iter()
            .find(|s| s.reorg_correction)
            .expect("Channel 2 corrective state not found");
        assert_eq!(
            corrective_2.balance,
            vec![4u8; 12],
            "Channel 2 should rollback to block 20 state"
        );

        // Verify channel 3 correction
        let states_3 = get_channel_states(&db, 3).await?;
        let corrective_3 = states_3
            .iter()
            .find(|s| s.reorg_correction)
            .expect("Channel 3 corrective state not found");
        assert_eq!(
            corrective_3.balance,
            vec![6u8; 12],
            "Channel 3 should rollback to block 30 state"
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_handle_reorg_channel_opened_during_reorg() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;

        // Create two channels
        create_test_channel(&db, 1).await?;
        create_test_channel(&db, 2).await?;

        // Channel 1: has state before reorg
        create_channel_state(&db, 1, 10, 0, 0, [1u8; 12], 1, false).await?;
        create_channel_state(&db, 1, 100, 0, 0, [2u8; 12], 2, false).await?;

        // Channel 2: ONLY has state during reorg (opened at block 95)
        create_channel_state(&db, 2, 95, 0, 0, [3u8; 12], 1, false).await?;

        // Simulate reorg affecting blocks 90-110
        let reorg_info = ReorgInfo {
            detected_at_block: 150,
            affected_block_range: (90, 110),
            removed_log_count: 2,
        };

        // Handle the reorg
        let corrected_count = Indexer::<MockHoprIndexerOps, MockChainLogHandler, BlokliDb>::handle_reorg(
            &db,
            &reorg_info,
            150,
            &IndexerState::default(),
        )
        .await?;

        // Only channel 1 should be corrected, channel 2 was opened during reorg
        assert_eq!(corrected_count, 1, "Only channel 1 should be corrected");

        // Verify channel 1 has corrective state
        let states_1 = get_channel_states(&db, 1).await?;
        assert!(
            states_1.iter().any(|s| s.reorg_correction),
            "Channel 1 should have corrective state"
        );

        // Verify channel 2 has NO corrective state
        let states_2 = get_channel_states(&db, 2).await?;
        assert!(
            !states_2.iter().any(|s| s.reorg_correction),
            "Channel 2 should NOT have corrective state"
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_handle_reorg_no_affected_channels() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;

        // Create channel with states outside reorg range
        create_test_channel(&db, 1).await?;
        create_channel_state(&db, 1, 10, 0, 0, [1u8; 12], 1, false).await?;
        create_channel_state(&db, 1, 50, 0, 0, [2u8; 12], 2, false).await?;

        // Simulate reorg affecting blocks 100-110 (no states in this range)
        let reorg_info = ReorgInfo {
            detected_at_block: 150,
            affected_block_range: (100, 110),
            removed_log_count: 0,
        };

        // Handle the reorg
        let corrected_count = Indexer::<MockHoprIndexerOps, MockChainLogHandler, BlokliDb>::handle_reorg(
            &db,
            &reorg_info,
            150,
            &IndexerState::default(),
        )
        .await?;

        // No channels should be corrected
        assert_eq!(corrected_count, 0, "No channels should be corrected");

        // Verify no corrective states were inserted
        let states = get_channel_states(&db, 1).await?;
        assert!(
            !states.iter().any(|s| s.reorg_correction),
            "No corrective states should be inserted"
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_handle_reorg_corrective_state_fields() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;

        create_test_channel(&db, 1).await?;

        // Create states with specific values to verify they're copied correctly
        create_channel_state(&db, 1, 50, 1, 2, [42u8; 12], 5, false).await?;
        create_channel_state(&db, 1, 100, 0, 0, [99u8; 12], 9, false).await?;

        let reorg_info = ReorgInfo {
            detected_at_block: 150,
            affected_block_range: (100, 100),
            removed_log_count: 1,
        };

        let corrected_count = Indexer::<MockHoprIndexerOps, MockChainLogHandler, BlokliDb>::handle_reorg(
            &db,
            &reorg_info,
            200,
            &IndexerState::default(),
        )
        .await?;

        assert_eq!(corrected_count, 1);

        let states = get_channel_states(&db, 1).await?;
        let corrective = states
            .iter()
            .find(|s| s.reorg_correction)
            .expect("Corrective state not found");

        // Verify all fields from last valid state are copied
        assert_eq!(
            corrective.balance,
            vec![42u8; 12],
            "Balance should match last valid state"
        );
        assert_eq!(corrective.status, 5, "Status should match last valid state");
        assert_eq!(corrective.channel_id, 1, "Channel ID should match");

        // Verify synthetic position fields
        assert_eq!(corrective.published_block, 200, "Should use canonical block");
        assert_eq!(corrective.published_tx_index, 0, "Should use synthetic tx_index 0");
        assert_eq!(corrective.published_log_index, 0, "Should use synthetic log_index 0");

        // Verify reorg correction flag
        assert!(corrective.reorg_correction, "reorg_correction flag should be true");

        Ok(())
    }

    #[tokio::test]
    async fn test_handle_reorg_preserves_audit_trail() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;

        create_test_channel(&db, 1).await?;

        // Create states
        create_channel_state(&db, 1, 10, 0, 0, [1u8; 12], 1, false).await?;
        create_channel_state(&db, 1, 50, 0, 0, [2u8; 12], 2, false).await?;
        create_channel_state(&db, 1, 100, 0, 0, [3u8; 12], 3, false).await?;

        let states_before = get_channel_states(&db, 1).await?;
        let count_before = states_before.len();

        let reorg_info = ReorgInfo {
            detected_at_block: 150,
            affected_block_range: (100, 100),
            removed_log_count: 1,
        };

        Indexer::<MockHoprIndexerOps, MockChainLogHandler, BlokliDb>::handle_reorg(
            &db,
            &reorg_info,
            200,
            &IndexerState::default(),
        )
        .await?;

        let states_after = get_channel_states(&db, 1).await?;

        // Verify audit trail is preserved - nothing deleted, only added
        assert_eq!(
            states_after.len(),
            count_before + 1,
            "Reorg should add corrective state, not delete"
        );

        // Verify all original states still exist
        assert!(
            states_after.iter().any(|s| s.published_block == 10),
            "Block 10 state should still exist"
        );
        assert!(
            states_after.iter().any(|s| s.published_block == 50),
            "Block 50 state should still exist"
        );
        assert!(
            states_after.iter().any(|s| s.published_block == 100),
            "Block 100 state should still exist"
        );

        // Verify corrective state was added
        assert!(
            states_after
                .iter()
                .any(|s| s.reorg_correction && s.published_block == 200),
            "Corrective state should be added"
        );

        Ok(())
    }

    // ==================== Edge Case Tests ====================

    #[tokio::test]
    async fn test_handle_reorg_large_block_range() -> anyhow::Result<()> {
        // Test reorg spanning many blocks to verify performance
        let db = BlokliDb::new_in_memory().await?;

        create_test_channel(&db, 1).await?;

        // Create states before reorg range
        create_channel_state(&db, 1, 10, 0, 0, [1u8; 12], 1, false).await?;

        // Create many states in reorg range (blocks 100-200)
        for block in 100..=200 {
            create_channel_state(&db, 1, block, 0, 0, [2u8; 12], 1, false).await?;
        }

        let reorg_info = ReorgInfo {
            detected_at_block: 250,
            affected_block_range: (100, 200),
            removed_log_count: 101,
        };

        let corrected_count = Indexer::<MockHoprIndexerOps, MockChainLogHandler, BlokliDb>::handle_reorg(
            &db,
            &reorg_info,
            250,
            &IndexerState::default(),
        )
        .await?;

        assert_eq!(
            corrected_count, 1,
            "Should correct one channel despite many affected blocks"
        );

        let states = get_channel_states(&db, 1).await?;
        let corrective = states
            .iter()
            .find(|s| s.reorg_correction)
            .expect("Corrective state not found");

        // Should use last state before reorg (block 10)
        assert_eq!(corrective.balance, vec![1u8; 12]);
        assert_eq!(corrective.published_block, 250);

        Ok(())
    }

    #[tokio::test]
    async fn test_handle_reorg_with_no_prior_state() -> anyhow::Result<()> {
        // Channel opened during reorg - should be skipped
        let db = BlokliDb::new_in_memory().await?;

        create_test_channel(&db, 1).await?;

        // Only create state in reorg range
        create_channel_state(&db, 1, 100, 0, 0, [1u8; 12], 1, false).await?;

        let reorg_info = ReorgInfo {
            detected_at_block: 150,
            affected_block_range: (95, 105),
            removed_log_count: 1,
        };

        let corrected_count = Indexer::<MockHoprIndexerOps, MockChainLogHandler, BlokliDb>::handle_reorg(
            &db,
            &reorg_info,
            150,
            &IndexerState::default(),
        )
        .await?;

        assert_eq!(
            corrected_count, 0,
            "Should skip channel with no prior state (opened during reorg)"
        );

        let states = get_channel_states(&db, 1).await?;
        assert!(
            !states.iter().any(|s| s.reorg_correction),
            "No corrective state should be added"
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_handle_reorg_same_block_different_positions() -> anyhow::Result<()> {
        // Test correct position ordering within same block
        let db = BlokliDb::new_in_memory().await?;

        create_test_channel(&db, 1).await?;

        // Create multiple states at same block with different positions
        create_channel_state(&db, 1, 50, 0, 5, [1u8; 12], 1, false).await?;
        create_channel_state(&db, 1, 50, 1, 0, [2u8; 12], 1, false).await?;
        create_channel_state(&db, 1, 50, 1, 3, [3u8; 12], 1, false).await?;

        // States in reorg range
        create_channel_state(&db, 1, 100, 0, 0, [99u8; 12], 2, false).await?;

        let reorg_info = ReorgInfo {
            detected_at_block: 150,
            affected_block_range: (100, 100),
            removed_log_count: 1,
        };

        Indexer::<MockHoprIndexerOps, MockChainLogHandler, BlokliDb>::handle_reorg(
            &db,
            &reorg_info,
            200,
            &IndexerState::default(),
        )
        .await?;

        let states = get_channel_states(&db, 1).await?;
        let corrective = states
            .iter()
            .find(|s| s.reorg_correction)
            .expect("Corrective state not found");

        // Should use latest state from block 50 (tx=1, log=3)
        assert_eq!(
            corrective.balance,
            vec![3u8; 12],
            "Should use state with highest position in block 50"
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_handle_reorg_watermark_boundary() -> anyhow::Result<()> {
        // Test reorg exactly at boundary conditions
        let db = BlokliDb::new_in_memory().await?;

        create_test_channel(&db, 1).await?;

        // State just before reorg start
        create_channel_state(&db, 1, 99, 9, 9, [1u8; 12], 1, false).await?;

        // State exactly at reorg start
        create_channel_state(&db, 1, 100, 0, 0, [2u8; 12], 2, false).await?;

        // State in middle of reorg range
        create_channel_state(&db, 1, 105, 0, 0, [3u8; 12], 2, false).await?;

        // State exactly at reorg end
        create_channel_state(&db, 1, 110, 0, 0, [4u8; 12], 2, false).await?;

        let reorg_info = ReorgInfo {
            detected_at_block: 150,
            affected_block_range: (100, 110),
            removed_log_count: 3,
        };

        Indexer::<MockHoprIndexerOps, MockChainLogHandler, BlokliDb>::handle_reorg(
            &db,
            &reorg_info,
            200,
            &IndexerState::default(),
        )
        .await?;

        let states = get_channel_states(&db, 1).await?;
        let corrective = states
            .iter()
            .find(|s| s.reorg_correction)
            .expect("Corrective state not found");

        // Should use last state before reorg (block 99)
        assert_eq!(corrective.balance, vec![1u8; 12]);
        assert_eq!(corrective.status, 1);

        Ok(())
    }

    #[tokio::test]
    async fn test_handle_reorg_duplicate_correction_attempt() -> anyhow::Result<()> {
        // Test idempotency - second correction should work or be benign
        let db = BlokliDb::new_in_memory().await?;

        create_test_channel(&db, 1).await?;

        create_channel_state(&db, 1, 50, 0, 0, [1u8; 12], 1, false).await?;
        create_channel_state(&db, 1, 100, 0, 0, [2u8; 12], 2, false).await?;

        let reorg_info = ReorgInfo {
            detected_at_block: 150,
            affected_block_range: (100, 100),
            removed_log_count: 1,
        };

        // First correction
        let count1 = Indexer::<MockHoprIndexerOps, MockChainLogHandler, BlokliDb>::handle_reorg(
            &db,
            &reorg_info,
            200,
            &IndexerState::default(),
        )
        .await?;
        assert_eq!(count1, 1);

        let states_after_first = get_channel_states(&db, 1).await?;
        let corrections_count = states_after_first.iter().filter(|s| s.reorg_correction).count();

        // Second correction attempt at different canonical block
        let count2 = Indexer::<MockHoprIndexerOps, MockChainLogHandler, BlokliDb>::handle_reorg(
            &db,
            &reorg_info,
            201,
            &IndexerState::default(),
        )
        .await?;
        assert_eq!(count2, 1, "Second correction should also succeed");

        let states_after_second = get_channel_states(&db, 1).await?;
        let corrections_count_2 = states_after_second.iter().filter(|s| s.reorg_correction).count();

        assert_eq!(
            corrections_count_2,
            corrections_count + 1,
            "Should add another corrective state"
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_handle_reorg_concurrent_channels() -> anyhow::Result<()> {
        // Test with many channels affected simultaneously
        let db = BlokliDb::new_in_memory().await?;

        let channel_count = 50; // Use moderate number for test speed

        // Create many channels with states
        for i in 1..=channel_count {
            create_test_channel(&db, i).await?;
            create_channel_state(&db, i, 50, 0, 0, [1u8; 12], 1, false).await?;
            create_channel_state(&db, i, 100, 0, 0, [2u8; 12], 2, false).await?;
        }

        let reorg_info = ReorgInfo {
            detected_at_block: 150,
            affected_block_range: (100, 100),
            removed_log_count: channel_count as usize,
        };

        let corrected_count = Indexer::<MockHoprIndexerOps, MockChainLogHandler, BlokliDb>::handle_reorg(
            &db,
            &reorg_info,
            200,
            &IndexerState::default(),
        )
        .await?;

        assert_eq!(
            corrected_count as i64, channel_count,
            "Should correct all affected channels"
        );

        // Verify each channel has corrective state
        for i in 1..=channel_count {
            let states = get_channel_states(&db, i).await?;
            let has_correction = states.iter().any(|s| s.reorg_correction);
            assert!(has_correction, "Channel {} should have corrective state", i);
        }

        Ok(())
    }

    #[tokio::test]
    async fn test_handle_reorg_with_reorg_correction_states() -> anyhow::Result<()> {
        // Channel already has reorg correction from previous reorg
        let db = BlokliDb::new_in_memory().await?;

        create_test_channel(&db, 1).await?;

        // Initial state
        create_channel_state(&db, 1, 50, 0, 0, [1u8; 12], 1, false).await?;

        // Previous reorg correction
        create_channel_state(&db, 1, 150, 0, 0, [2u8; 12], 1, true).await?;

        // New states after first reorg
        create_channel_state(&db, 1, 200, 0, 0, [3u8; 12], 2, false).await?;

        // Second reorg affecting block 200
        let reorg_info = ReorgInfo {
            detected_at_block: 250,
            affected_block_range: (200, 200),
            removed_log_count: 1,
        };

        Indexer::<MockHoprIndexerOps, MockChainLogHandler, BlokliDb>::handle_reorg(
            &db,
            &reorg_info,
            250,
            &IndexerState::default(),
        )
        .await?;

        let states = get_channel_states(&db, 1).await?;

        // Should now have 2 corrective states
        let correction_count = states.iter().filter(|s| s.reorg_correction).count();
        assert_eq!(correction_count, 2, "Should have both reorg corrections");

        // Latest correction should reference previous correction state
        let latest_correction = states
            .iter()
            .filter(|s| s.reorg_correction)
            .max_by_key(|s| s.published_block)
            .unwrap();

        assert_eq!(
            latest_correction.balance,
            vec![2u8; 12],
            "Should use previous correction as base"
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_temporal_query_at_reorg_boundary() -> anyhow::Result<()> {
        // Test querying at exact reorg correction position
        let db = BlokliDb::new_in_memory().await?;

        create_test_channel(&db, 1).await?;

        create_channel_state(&db, 1, 50, 0, 0, [1u8; 12], 1, false).await?;
        create_channel_state(&db, 1, 100, 0, 0, [2u8; 12], 2, false).await?;

        let reorg_info = ReorgInfo {
            detected_at_block: 150,
            affected_block_range: (100, 100),
            removed_log_count: 1,
        };

        Indexer::<MockHoprIndexerOps, MockChainLogHandler, BlokliDb>::handle_reorg(
            &db,
            &reorg_info,
            200,
            &IndexerState::default(),
        )
        .await?;

        let conn = db.conn(TargetDb::Index);

        // Query at exact corrective state position (200, 0, 0)
        let position = BlockPosition {
            block: 200,
            tx_index: 0,
            log_index: 0,
        };

        let state = get_channel_state_at(conn, 1, position).await?;
        assert!(state.is_some(), "Should find corrective state");

        let found_state = state.unwrap();
        assert_eq!(
            found_state.balance,
            vec![1u8; 12],
            "Should return corrective state balance"
        );
        assert!(found_state.reorg_correction, "Should be reorg correction state");
        assert_eq!(found_state.published_block, 200, "Should be at synthetic position");
        assert_eq!(found_state.published_tx_index, 0, "Should use synthetic tx_index 0");
        assert_eq!(found_state.published_log_index, 0, "Should use synthetic log_index 0");

        Ok(())
    }
}
