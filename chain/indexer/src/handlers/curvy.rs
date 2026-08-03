use blokli_api_types::{
    CurvyCommitmentGasCostUpdate, CurvyCommitmentGasFeeRootUpdate, CurvyCommittedNote, CurvyCommittedNullifier,
    CurvyEventPosition, CurvyGasFees, CurvyPendingNote, CurvyTokenRegistration, Hex32, UInt64, UInt256,
};
use blokli_chain_rpc::Log;
use blokli_chain_types::AlloyAddressExt;
use blokli_db::{BlokliDbAllOperations, OpenTransaction, errors::DbSqlError};
use blokli_db_entity::{
    curvy_commitment_gas_cost, curvy_commitment_gas_fee_root, curvy_committed_note, curvy_committed_nullifier,
    curvy_pending_note, curvy_token_registration,
};
use curvy_bindings::{
    curvy_aggregator_alpha_v2::CurvyAggregatorAlphaV2::{CurvyAggregatorAlphaV2Events, PendingNotes},
    curvy_vault_v2::CurvyVaultV2::CurvyVaultV2Events,
};
use hopr_bindings::exports::alloy::primitives::U256;
use hopr_types::primitive::traits::ToHex;
use sea_orm::sea_query::OnConflict;
use sea_orm::{ActiveValue::Set, EntityTrait};

use super::ContractEventHandlers;
use crate::{errors::CoreEthereumIndexerError, errors::Result, state::IndexerEvent};

fn u256_bytes(value: U256) -> Vec<u8> {
    value.to_be_bytes::<32>().to_vec()
}

fn u256_scalar(value: U256) -> UInt256 {
    UInt256(value.to_string())
}

fn position(log: &Log, event_item_index: i64) -> Result<CurvyEventPosition> {
    Ok(CurvyEventPosition {
        transaction_hash: Hex32(log.tx_hash.to_hex()),
        block: UInt64(log.block_number),
        transaction_index: UInt64(log.tx_index),
        log_index: UInt64(log.log_index.as_u64()),
        event_item_index: UInt64(
            u64::try_from(event_item_index)
                .map_err(|_| CoreEthereumIndexerError::ProcessError("Curvy event item index underflow".to_string()))?,
        ),
    })
}

fn coordinates(log: &Log) -> Result<(i64, i64, i64)> {
    Ok((
        i64::try_from(log.block_number)
            .map_err(|_| CoreEthereumIndexerError::ProcessError("Curvy event block number overflow".to_string()))?,
        i64::try_from(log.tx_index).map_err(|_| {
            CoreEthereumIndexerError::ProcessError("Curvy event transaction index overflow".to_string())
        })?,
        i64::try_from(log.log_index.as_u64())
            .map_err(|_| CoreEthereumIndexerError::ProcessError("Curvy event log index overflow".to_string()))?,
    ))
}

fn validate_pending_notes(event: &PendingNotes) -> Result<usize> {
    let len = event.noteIds.len();
    if event.ephemeralKeys[0].len() != len
        || event.ephemeralKeys[1].len() != len
        || event.viewTags.len() != len
        || event.tokens.len() != len
        || event.amounts.len() != len
        || event.isPlaintext.len() != len
    {
        return Err(CoreEthereumIndexerError::ProcessError(
            "PendingNotes contains arrays with different lengths".to_string(),
        ));
    }
    Ok(len)
}

impl<T, Db> ContractEventHandlers<T, Db>
where
    Db: BlokliDbAllOperations + Clone,
{
    pub(super) async fn on_curvy_aggregator_event(
        &self,
        tx: &OpenTransaction,
        log: &Log,
        event: CurvyAggregatorAlphaV2Events,
    ) -> Result<Vec<IndexerEvent>> {
        let (block, tx_index, log_index) = coordinates(log)?;
        let chain_tx_hash = log.tx_hash.as_ref().to_vec();

        match event {
            CurvyAggregatorAlphaV2Events::PendingNotes(event) => {
                let len = validate_pending_notes(&event)?;
                if len == 0 {
                    return Ok(Vec::new());
                }
                let mut models = Vec::with_capacity(len);
                let mut events = Vec::with_capacity(len);
                for item_index in 0..len {
                    let event_item_index = i64::try_from(item_index).map_err(|_| {
                        CoreEthereumIndexerError::ProcessError("PendingNotes item index overflow".to_string())
                    })?;
                    models.push(curvy_pending_note::ActiveModel {
                        note_id: Set(u256_bytes(event.noteIds[item_index])),
                        ephemeral_key_x: Set(u256_bytes(event.ephemeralKeys[0][item_index])),
                        ephemeral_key_y: Set(u256_bytes(event.ephemeralKeys[1][item_index])),
                        view_tag: Set(i64::from(event.viewTags[item_index])),
                        token_id: Set(u256_bytes(event.tokens[item_index])),
                        amount: Set(u256_bytes(event.amounts[item_index])),
                        is_plaintext: Set(event.isPlaintext[item_index]),
                        event_item_index: Set(event_item_index),
                        chain_tx_hash: Set(chain_tx_hash.clone()),
                        published_block: Set(block),
                        published_tx_index: Set(tx_index),
                        published_log_index: Set(log_index),
                        ..Default::default()
                    });
                    events.push(IndexerEvent::CurvyPendingNote(CurvyPendingNote {
                        note_id: u256_scalar(event.noteIds[item_index]),
                        ephemeral_key: vec![
                            u256_scalar(event.ephemeralKeys[0][item_index]),
                            u256_scalar(event.ephemeralKeys[1][item_index]),
                        ],
                        view_tag: i32::from(event.viewTags[item_index]),
                        token_id: u256_scalar(event.tokens[item_index]),
                        amount: u256_scalar(event.amounts[item_index]),
                        is_plaintext: event.isPlaintext[item_index],
                        position: position(log, event_item_index)?,
                    }));
                }
                curvy_pending_note::Entity::insert_many(models)
                    .on_conflict(
                        OnConflict::columns([
                            curvy_pending_note::Column::PublishedBlock,
                            curvy_pending_note::Column::PublishedTxIndex,
                            curvy_pending_note::Column::PublishedLogIndex,
                            curvy_pending_note::Column::EventItemIndex,
                        ])
                        .do_nothing()
                        .to_owned(),
                    )
                    .exec_without_returning(tx.as_ref())
                    .await
                    .map_err(DbSqlError::from)?;
                Ok(events)
            }
            CurvyAggregatorAlphaV2Events::CommittedNotes(event) => {
                let mut models = Vec::with_capacity(event.noteIds.len());
                let mut events = Vec::with_capacity(event.noteIds.len());
                for (item_index, note_id) in event.noteIds.into_iter().enumerate() {
                    let event_item_index = i64::try_from(item_index).map_err(|_| {
                        CoreEthereumIndexerError::ProcessError("CommittedNotes item index overflow".to_string())
                    })?;
                    models.push(curvy_committed_note::ActiveModel {
                        batch_index: Set(u256_bytes(event.batchIndex)),
                        note_id: Set(u256_bytes(note_id)),
                        event_item_index: Set(event_item_index),
                        chain_tx_hash: Set(chain_tx_hash.clone()),
                        published_block: Set(block),
                        published_tx_index: Set(tx_index),
                        published_log_index: Set(log_index),
                        ..Default::default()
                    });
                    events.push(IndexerEvent::CurvyCommittedNote(CurvyCommittedNote {
                        batch_index: u256_scalar(event.batchIndex),
                        note_id: u256_scalar(note_id),
                        position: position(log, event_item_index)?,
                    }));
                }
                if !models.is_empty() {
                    curvy_committed_note::Entity::insert_many(models)
                        .on_conflict(
                            OnConflict::columns([
                                curvy_committed_note::Column::PublishedBlock,
                                curvy_committed_note::Column::PublishedTxIndex,
                                curvy_committed_note::Column::PublishedLogIndex,
                                curvy_committed_note::Column::EventItemIndex,
                            ])
                            .do_nothing()
                            .to_owned(),
                        )
                        .exec_without_returning(tx.as_ref())
                        .await
                        .map_err(DbSqlError::from)?;
                }
                Ok(events)
            }
            CurvyAggregatorAlphaV2Events::CommittedNullifiers(event) => {
                let mut models = Vec::with_capacity(event.nullifiers.len());
                let mut events = Vec::with_capacity(event.nullifiers.len());
                for (item_index, nullifier) in event.nullifiers.into_iter().enumerate() {
                    let event_item_index = i64::try_from(item_index).map_err(|_| {
                        CoreEthereumIndexerError::ProcessError("CommittedNullifiers item index overflow".to_string())
                    })?;
                    models.push(curvy_committed_nullifier::ActiveModel {
                        batch_index: Set(u256_bytes(event.batchIndex)),
                        nullifier: Set(u256_bytes(nullifier)),
                        event_item_index: Set(event_item_index),
                        chain_tx_hash: Set(chain_tx_hash.clone()),
                        published_block: Set(block),
                        published_tx_index: Set(tx_index),
                        published_log_index: Set(log_index),
                        ..Default::default()
                    });
                    events.push(IndexerEvent::CurvyCommittedNullifier(CurvyCommittedNullifier {
                        batch_index: u256_scalar(event.batchIndex),
                        nullifier: u256_scalar(nullifier),
                        position: position(log, event_item_index)?,
                    }));
                }
                if !models.is_empty() {
                    curvy_committed_nullifier::Entity::insert_many(models)
                        .on_conflict(
                            OnConflict::columns([
                                curvy_committed_nullifier::Column::PublishedBlock,
                                curvy_committed_nullifier::Column::PublishedTxIndex,
                                curvy_committed_nullifier::Column::PublishedLogIndex,
                                curvy_committed_nullifier::Column::EventItemIndex,
                            ])
                            .do_nothing()
                            .to_owned(),
                        )
                        .exec_without_returning(tx.as_ref())
                        .await
                        .map_err(DbSqlError::from)?;
                }
                Ok(events)
            }
            CurvyAggregatorAlphaV2Events::CommitmentGasFeeRootUpdated(event) => {
                curvy_commitment_gas_fee_root::Entity::insert(curvy_commitment_gas_fee_root::ActiveModel {
                    root: Set(u256_bytes(event.root)),
                    chain_tx_hash: Set(chain_tx_hash),
                    published_block: Set(block),
                    published_tx_index: Set(tx_index),
                    published_log_index: Set(log_index),
                    ..Default::default()
                })
                .on_conflict(
                    OnConflict::columns([
                        curvy_commitment_gas_fee_root::Column::PublishedBlock,
                        curvy_commitment_gas_fee_root::Column::PublishedTxIndex,
                        curvy_commitment_gas_fee_root::Column::PublishedLogIndex,
                    ])
                    .do_nothing()
                    .to_owned(),
                )
                .exec_without_returning(tx.as_ref())
                .await
                .map_err(DbSqlError::from)?;
                Ok(vec![IndexerEvent::CurvyCommitmentGasFeeRootUpdated(
                    CurvyCommitmentGasFeeRootUpdate {
                        root: u256_scalar(event.root),
                        position: position(log, 0)?,
                    },
                )])
            }
            _ => Ok(Vec::new()),
        }
    }

    pub(super) async fn on_curvy_vault_event(
        &self,
        tx: &OpenTransaction,
        log: &Log,
        event: CurvyVaultV2Events,
    ) -> Result<Vec<IndexerEvent>> {
        let (block, tx_index, log_index) = coordinates(log)?;
        let chain_tx_hash = log.tx_hash.as_ref().to_vec();

        match event {
            CurvyVaultV2Events::TokenRegistration(event) => {
                curvy_token_registration::Entity::insert(curvy_token_registration::ActiveModel {
                    token_address: Set(event.token_address.as_slice().to_vec()),
                    token_id: Set(u256_bytes(event.token_id)),
                    chain_tx_hash: Set(chain_tx_hash),
                    published_block: Set(block),
                    published_tx_index: Set(tx_index),
                    published_log_index: Set(log_index),
                    ..Default::default()
                })
                .on_conflict(
                    OnConflict::columns([
                        curvy_token_registration::Column::PublishedBlock,
                        curvy_token_registration::Column::PublishedTxIndex,
                        curvy_token_registration::Column::PublishedLogIndex,
                    ])
                    .do_nothing()
                    .to_owned(),
                )
                .exec_without_returning(tx.as_ref())
                .await
                .map_err(DbSqlError::from)?;
                Ok(vec![IndexerEvent::CurvyTokenRegistered(CurvyTokenRegistration {
                    token_address: event.token_address.to_hopr_address().to_hex(),
                    token_id: u256_scalar(event.token_id),
                    position: position(log, 0)?,
                })])
            }
            CurvyVaultV2Events::CommitmentGasCostsUpdated(event) => {
                let mut models = Vec::with_capacity(event.gasFees.len());
                let mut events = Vec::with_capacity(event.gasFees.len());
                for (item_index, fees) in event.gasFees.into_iter().enumerate() {
                    let event_item_index = i64::try_from(item_index).map_err(|_| {
                        CoreEthereumIndexerError::ProcessError(
                            "CommitmentGasCostsUpdated item index overflow".to_string(),
                        )
                    })?;
                    models.push(curvy_commitment_gas_cost::ActiveModel {
                        token_id: Set(u256_bytes(fees.tokenId)),
                        portal_deployment: Set(u256_bytes(fees.portalDeployment)),
                        pending_note_commitment: Set(u256_bytes(fees.pendingNoteCommitment)),
                        withdrawal: Set(u256_bytes(fees.withdrawal)),
                        root: Set(u256_bytes(event.root)),
                        event_item_index: Set(event_item_index),
                        chain_tx_hash: Set(chain_tx_hash.clone()),
                        published_block: Set(block),
                        published_tx_index: Set(tx_index),
                        published_log_index: Set(log_index),
                        ..Default::default()
                    });
                    events.push(IndexerEvent::CurvyCommitmentGasCostsUpdated(
                        CurvyCommitmentGasCostUpdate {
                            gas_fees: CurvyGasFees {
                                token_id: u256_scalar(fees.tokenId),
                                portal_deployment: u256_scalar(fees.portalDeployment),
                                pending_note_commitment: u256_scalar(fees.pendingNoteCommitment),
                                withdrawal: u256_scalar(fees.withdrawal),
                            },
                            root: u256_scalar(event.root),
                            position: position(log, event_item_index)?,
                        },
                    ));
                }
                if !models.is_empty() {
                    curvy_commitment_gas_cost::Entity::insert_many(models)
                        .on_conflict(
                            OnConflict::columns([
                                curvy_commitment_gas_cost::Column::PublishedBlock,
                                curvy_commitment_gas_cost::Column::PublishedTxIndex,
                                curvy_commitment_gas_cost::Column::PublishedLogIndex,
                                curvy_commitment_gas_cost::Column::EventItemIndex,
                            ])
                            .do_nothing()
                            .to_owned(),
                        )
                        .exec_without_returning(tx.as_ref())
                        .await
                        .map_err(DbSqlError::from)?;
                }
                Ok(events)
            }
            _ => Ok(Vec::new()),
        }
    }
}

#[cfg(test)]
mod tests {
    use curvy_bindings::curvy_aggregator_alpha_v2::CurvyAggregatorAlphaV2::PendingNotes;
    use hopr_bindings::exports::alloy::primitives::U256;

    use super::validate_pending_notes;

    fn pending_notes() -> PendingNotes {
        PendingNotes {
            noteIds: vec![U256::from(1)],
            ephemeralKeys: [vec![U256::from(2)], vec![U256::from(3)]],
            viewTags: vec![4],
            tokens: vec![U256::from(5)],
            amounts: vec![U256::from(6)],
            isPlaintext: vec![true],
        }
    }

    #[test]
    fn test_pending_notes_accepts_aligned_arrays() {
        assert_eq!(validate_pending_notes(&pending_notes()).unwrap(), 1);
    }

    #[test]
    fn test_pending_notes_rejects_misaligned_arrays() {
        let mut event = pending_notes();
        event.amounts.clear();

        assert!(validate_pending_notes(&event).is_err());
    }
}
