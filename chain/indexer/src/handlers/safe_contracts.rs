use blokli_chain_rpc::Log;
use blokli_chain_types::AlloyAddressExt;
use blokli_db::{BlokliDbAllOperations, OpenTransaction, TargetDb, safe_history::SafeActivityKind};
use hopr_bindings::exports::alloy::{
    primitives::{Address as AlloyAddress, B256, Log as AlloyLog},
    sol_types::SolEventInterface,
};
use hopr_types::{
    crypto::types::Hash,
    primitive::prelude::{Address, SerializableLog},
};
use sea_orm::{ConnectionTrait, DatabaseBackend, QueryResult, Statement};
use tracing::{debug, info, warn};

use super::ContractEventHandlers;
use crate::{custom_abis::safe_contract_events::SafeContract::SafeContractEvents, errors::Result, state::IndexerEvent};

impl<T, Db> ContractEventHandlers<T, Db>
where
    T: blokli_chain_rpc::HoprIndexerRpcOperations + Clone + Send + 'static,
    Db: BlokliDbAllOperations + Clone,
{
    fn safe_log_replay_statement(
        backend: DatabaseBackend,
        safe_address: Address,
        block: u64,
        tx_index: u64,
        log_index: u64,
    ) -> Statement {
        let placeholder = |position: usize| {
            if backend == DatabaseBackend::Postgres {
                format!("${position}")
            } else {
                "?".to_string()
            }
        };

        let sql = format!(
            "SELECT address, topics, data, block_number, transaction_hash, tx_index, block_hash, log_index, removed
             FROM log
             WHERE address = {}
               AND removed = FALSE
               AND (
                    block_number < {}
                    OR (block_number = {} AND tx_index < {})
                    OR (block_number = {} AND tx_index = {} AND log_index < {})
               )
             ORDER BY block_number ASC, tx_index ASC, log_index ASC",
            placeholder(1),
            placeholder(2),
            placeholder(3),
            placeholder(4),
            placeholder(5),
            placeholder(6),
            placeholder(7),
        );

        Statement::from_sql_and_values(
            backend,
            sql,
            vec![
                safe_address.as_ref().to_vec().into(),
                (block as i64).into(),
                (block as i64).into(),
                (tx_index as i64).into(),
                (block as i64).into(),
                (tx_index as i64).into(),
                (log_index as i64).into(),
            ],
        )
    }

    fn serializable_log_from_row(row: QueryResult) -> Result<SerializableLog> {
        let address_bytes: Vec<u8> = row.try_get("", "address").map_err(|e| {
            crate::errors::CoreEthereumIndexerError::ProcessError(format!("failed to read address from log db: {e}"))
        })?;
        let topics_bytes: Vec<u8> = row.try_get("", "topics").map_err(|e| {
            crate::errors::CoreEthereumIndexerError::ProcessError(format!("failed to read topics from log db: {e}"))
        })?;
        let tx_hash_bytes: Vec<u8> = row.try_get("", "transaction_hash").map_err(|e| {
            crate::errors::CoreEthereumIndexerError::ProcessError(format!(
                "failed to read transaction hash from log db: {e}"
            ))
        })?;
        let block_hash_bytes: Vec<u8> = row.try_get("", "block_hash").map_err(|e| {
            crate::errors::CoreEthereumIndexerError::ProcessError(format!("failed to read block hash from log db: {e}"))
        })?;

        let topics = topics_bytes
            .chunks_exact(32)
            .map(|chunk| {
                let mut topic = [0u8; 32];
                topic.copy_from_slice(chunk);
                topic
            })
            .collect::<Vec<_>>();

        let tx_hash: [u8; 32] = tx_hash_bytes.try_into().map_err(|_| {
            crate::errors::CoreEthereumIndexerError::ProcessError("invalid tx hash length in log db".into())
        })?;
        let block_hash: [u8; 32] = block_hash_bytes.try_into().map_err(|_| {
            crate::errors::CoreEthereumIndexerError::ProcessError("invalid block hash length in log db".into())
        })?;

        let block_number: i64 = row.try_get("", "block_number").map_err(|e| {
            crate::errors::CoreEthereumIndexerError::ProcessError(format!(
                "failed to read block number from log db: {e}"
            ))
        })?;
        let tx_index: i64 = row.try_get("", "tx_index").map_err(|e| {
            crate::errors::CoreEthereumIndexerError::ProcessError(format!("failed to read tx index from log db: {e}"))
        })?;
        let log_index: i64 = row.try_get("", "log_index").map_err(|e| {
            crate::errors::CoreEthereumIndexerError::ProcessError(format!("failed to read log index from log db: {e}"))
        })?;

        Ok(SerializableLog {
            address: Address::new(address_bytes.as_slice()),
            topics,
            data: row.try_get("", "data").map_err(|e| {
                crate::errors::CoreEthereumIndexerError::ProcessError(format!("failed to read data from log db: {e}"))
            })?,
            block_number: block_number as u64,
            tx_hash,
            tx_index: tx_index as u64,
            block_hash,
            log_index: log_index as u64,
            removed: row.try_get("", "removed").map_err(|e| {
                crate::errors::CoreEthereumIndexerError::ProcessError(format!("failed to read removed flag: {e}"))
            })?,
            ..Default::default()
        })
    }

    pub(super) async fn replay_safe_logs_before_position(
        &self,
        tx: &OpenTransaction,
        safe_address: Address,
        block: u64,
        tx_index: u64,
        log_index: u64,
    ) -> Result<()> {
        let backend = self.db.conn(TargetDb::Logs).get_database_backend();
        let rows = self
            .db
            .conn(TargetDb::Logs)
            .query_all_raw(Self::safe_log_replay_statement(
                backend,
                safe_address,
                block,
                tx_index,
                log_index,
            ))
            .await
            .map_err(|e| {
                crate::errors::CoreEthereumIndexerError::ProcessError(format!(
                    "failed to query stored Safe logs for replay: {e}"
                ))
            })?;

        for row in rows {
            let slog = Self::serializable_log_from_row(row)?;
            if !slog.topics.first().is_some_and(Self::is_safe_contract_topic) {
                continue;
            }

            let primitive_log = AlloyLog::new(
                AlloyAddress::from_hopr_address(slog.address),
                slog.topics.iter().map(|hash| B256::from_slice(hash.as_ref())).collect(),
                slog.data.clone().into(),
            )
            .ok_or_else(|| {
                crate::errors::CoreEthereumIndexerError::ProcessError(format!(
                    "failed to convert replayed safe log to primitive log: {slog:?}"
                ))
            })?;

            let event = SafeContractEvents::decode_log(&primitive_log)?;
            let log = Log::from(slog);
            debug!(
                safe_address = %safe_address,
                block = log.block_number,
                tx_index = log.tx_index,
                log_index = %log.log_index,
                "Replaying stored Safe log after Safe discovery"
            );
            self.on_safe_contract_event(tx, safe_address, &log, event.data, false)
                .await?;
        }

        Ok(())
    }

    pub(super) async fn on_safe_contract_event(
        &self,
        tx: &OpenTransaction,
        safe_address: hopr_types::primitive::prelude::Address,
        log: &Log,
        event: SafeContractEvents,
        _is_synced: bool,
    ) -> Result<Vec<IndexerEvent>> {
        let chain_tx_hash = Hash::from(log.tx_hash);

        match event {
            SafeContractEvents::SafeSetup(safe_setup) => {
                let owner_count = safe_setup.owners.len();
                self.db
                    .record_safe_activity(
                        Some(tx),
                        safe_address,
                        SafeActivityKind::SafeSetup,
                        chain_tx_hash,
                        None,
                        None,
                        Some(safe_setup.threshold.to_string()),
                        None,
                        Some(safe_setup.initiator.to_hopr_address()),
                        log.block_number,
                        log.tx_index,
                        log.log_index.as_u64(),
                    )
                    .await?;

                for owner in safe_setup.owners {
                    let owner = owner.to_hopr_address();
                    self.db
                        .upsert_safe_owner_state(
                            Some(tx),
                            safe_address,
                            owner,
                            true,
                            log.block_number,
                            log.tx_index,
                            log.log_index.as_u64(),
                        )
                        .await?;
                }

                info!(
                    safe_address = %safe_address,
                    owner_count,
                    threshold = %safe_setup.threshold.to_string(),
                    block = log.block_number,
                    tx_index = log.tx_index,
                    log_index = %log.log_index,
                    "Persisted Safe SafeSetup event"
                );
            }
            SafeContractEvents::AddedOwner(added_owner) => {
                let owner = added_owner.owner.to_hopr_address();
                self.db
                    .record_safe_activity(
                        Some(tx),
                        safe_address,
                        SafeActivityKind::AddedOwner,
                        chain_tx_hash,
                        None,
                        Some(owner),
                        None,
                        None,
                        None,
                        log.block_number,
                        log.tx_index,
                        log.log_index.as_u64(),
                    )
                    .await?;
                self.db
                    .upsert_safe_owner_state(
                        Some(tx),
                        safe_address,
                        owner,
                        true,
                        log.block_number,
                        log.tx_index,
                        log.log_index.as_u64(),
                    )
                    .await?;
                info!(
                    safe_address = %safe_address,
                    owner = %owner,
                    block = log.block_number,
                    tx_index = log.tx_index,
                    log_index = %log.log_index,
                    "Persisted Safe AddedOwner event"
                );
            }
            SafeContractEvents::RemovedOwner(removed_owner) => {
                let owner = removed_owner.owner.to_hopr_address();
                self.db
                    .record_safe_activity(
                        Some(tx),
                        safe_address,
                        SafeActivityKind::RemovedOwner,
                        chain_tx_hash,
                        None,
                        Some(owner),
                        None,
                        None,
                        None,
                        log.block_number,
                        log.tx_index,
                        log.log_index.as_u64(),
                    )
                    .await?;
                self.db
                    .upsert_safe_owner_state(
                        Some(tx),
                        safe_address,
                        owner,
                        false,
                        log.block_number,
                        log.tx_index,
                        log.log_index.as_u64(),
                    )
                    .await?;
                info!(
                    safe_address = %safe_address,
                    owner = %owner,
                    block = log.block_number,
                    tx_index = log.tx_index,
                    log_index = %log.log_index,
                    "Persisted Safe RemovedOwner event"
                );
            }
            SafeContractEvents::ChangedThreshold(changed_threshold) => {
                self.db
                    .record_safe_activity(
                        Some(tx),
                        safe_address,
                        SafeActivityKind::ChangedThreshold,
                        chain_tx_hash,
                        None,
                        None,
                        Some(changed_threshold.threshold.to_string()),
                        None,
                        None,
                        log.block_number,
                        log.tx_index,
                        log.log_index.as_u64(),
                    )
                    .await?;
                info!(
                    safe_address = %safe_address,
                    threshold = %changed_threshold.threshold.to_string(),
                    block = log.block_number,
                    tx_index = log.tx_index,
                    log_index = %log.log_index,
                    "Persisted Safe ChangedThreshold event"
                );
            }
            SafeContractEvents::ExecutionSuccess(execution) => {
                self.db
                    .record_safe_activity(
                        Some(tx),
                        safe_address,
                        SafeActivityKind::ExecutionSuccess,
                        chain_tx_hash,
                        Some(Hash::from(execution.txHash.0)),
                        None,
                        None,
                        Some(execution.payment.to_string()),
                        None,
                        log.block_number,
                        log.tx_index,
                        log.log_index.as_u64(),
                    )
                    .await?;
                info!(
                    safe_address = %safe_address,
                    safe_tx_hash = %Hash::from(execution.txHash.0),
                    payment = %execution.payment.to_string(),
                    block = log.block_number,
                    tx_index = log.tx_index,
                    log_index = %log.log_index,
                    "Persisted Safe ExecutionSuccess event"
                );
            }
            SafeContractEvents::ExecutionFailure(execution) => {
                self.db
                    .record_safe_activity(
                        Some(tx),
                        safe_address,
                        SafeActivityKind::ExecutionFailure,
                        chain_tx_hash,
                        Some(Hash::from(execution.txHash.0)),
                        None,
                        None,
                        Some(execution.payment.to_string()),
                        None,
                        log.block_number,
                        log.tx_index,
                        log.log_index.as_u64(),
                    )
                    .await?;
                warn!(
                    safe_address = %safe_address,
                    safe_tx_hash = %Hash::from(execution.txHash.0),
                    payment = %execution.payment.to_string(),
                    block = log.block_number,
                    tx_index = log.tx_index,
                    log_index = %log.log_index,
                    "Persisted Safe ExecutionFailure event"
                );
            }
        }

        Ok(Vec::new())
    }
}
