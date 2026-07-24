use std::collections::{HashMap, HashSet};

use async_trait::async_trait;
use blokli_db_entity::{
    hopr_safe_contract, hopr_safe_event, hopr_safe_execution_event, hopr_safe_owner_change_event,
    hopr_safe_owner_state, hopr_safe_setup_event, hopr_safe_setup_owner, hopr_safe_threshold_change_event,
    hopr_safe_threshold_state,
    prelude::{
        HoprSafeContract, HoprSafeEvent, HoprSafeExecutionEvent, HoprSafeOwnerChangeEvent, HoprSafeOwnerState,
        HoprSafeSetupEvent, HoprSafeSetupOwner, HoprSafeThresholdChangeEvent, HoprSafeThresholdState,
    },
};
use hopr_types::{
    crypto::types::Hash,
    primitive::prelude::{Address, ToHex},
};
use sea_orm::{ColumnTrait, ConnectionTrait, DbBackend, DbErr, EntityTrait, QueryFilter, QueryOrder, Set};
use sea_query::{Condition, OnConflict};

use crate::{
    BlokliDb, BlokliDbGeneralModelOperations, DbSqlError, OptTx, Result, TargetDb,
    safe_contracts::BlokliDbSafeContractOperations, with_opt_transaction,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SafeActivityKind {
    SafeSetup,
    AddedOwner,
    RemovedOwner,
    ChangedThreshold,
    ExecutionSuccess,
    ExecutionFailure,
}

impl SafeActivityKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::SafeSetup => "SAFE_SETUP",
            Self::AddedOwner => "ADDED_OWNER",
            Self::RemovedOwner => "REMOVED_OWNER",
            Self::ChangedThreshold => "CHANGED_THRESHOLD",
            Self::ExecutionSuccess => "EXECUTION_SUCCESS",
            Self::ExecutionFailure => "EXECUTION_FAILURE",
        }
    }

    fn from_str(value: &str) -> Result<Self> {
        match value {
            "SAFE_SETUP" => Ok(Self::SafeSetup),
            "ADDED_OWNER" => Ok(Self::AddedOwner),
            "REMOVED_OWNER" => Ok(Self::RemovedOwner),
            "CHANGED_THRESHOLD" => Ok(Self::ChangedThreshold),
            "EXECUTION_SUCCESS" => Ok(Self::ExecutionSuccess),
            "EXECUTION_FAILURE" => Ok(Self::ExecutionFailure),
            _ => Err(DbSqlError::Construction(format!("unknown safe activity kind: {value}"))),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SafeActivityEntry {
    pub safe_address: Vec<u8>,
    pub event_kind: String,
    pub chain_tx_hash: Vec<u8>,
    pub safe_tx_hash: Option<Vec<u8>>,
    pub owner_address: Option<Vec<u8>>,
    pub owners: Vec<Vec<u8>>,
    pub threshold: Option<String>,
    pub payment: Option<String>,
    pub initiator_address: Option<Vec<u8>>,
    pub published_block: i64,
    pub published_tx_index: i64,
    pub published_log_index: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StagedSafeMutationKind {
    SafeSetup {
        owners: Vec<Address>,
        threshold: String,
        initiator_address: Option<Address>,
    },
    AddedOwner {
        owner_address: Address,
    },
    RemovedOwner {
        owner_address: Address,
    },
    ChangedThreshold {
        threshold: String,
    },
    ExecutionSuccess {
        safe_tx_hash: Hash,
        payment: String,
    },
    ExecutionFailure {
        safe_tx_hash: Hash,
        payment: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StagedSafeMutation {
    pub safe_address: Address,
    pub chain_tx_hash: Hash,
    pub block: u64,
    pub tx_index: u64,
    pub log_index: u64,
    pub event: StagedSafeMutationKind,
}

#[derive(Clone, Debug)]
struct PreparedSafeMutation {
    safe_address: Address,
    safe_contract_id: i64,
    chain_tx_hash: Hash,
    block: i64,
    tx_index: i64,
    log_index: i64,
    event: StagedSafeMutationKind,
}

#[derive(Clone, Debug)]
struct SafeEventHeader {
    event_id: i64,
    event_kind: SafeActivityKind,
    chain_tx_hash: Vec<u8>,
    published_block: i64,
    published_tx_index: i64,
    published_log_index: i64,
}

#[derive(Clone, Debug)]
struct SafeSetupDetail {
    initiator_address: Option<Vec<u8>>,
    threshold: String,
}

#[derive(Clone, Debug)]
struct SafeOwnerChangeDetail {
    owner_address: Vec<u8>,
}

#[derive(Clone, Debug)]
struct SafeThresholdChangeDetail {
    threshold: String,
}

#[derive(Clone, Debug)]
struct SafeExecutionDetail {
    safe_tx_hash: Vec<u8>,
    payment: String,
}

type SafeEventIdentity = (i64, i64, i64, i64);

const SQLITE_SAFE_HISTORY_BATCH_SIZE: usize = 100;
const DEFAULT_SAFE_HISTORY_BATCH_SIZE: usize = 1_000;

fn safe_history_batch_size(backend: DbBackend) -> usize {
    match backend {
        DbBackend::Sqlite => SQLITE_SAFE_HISTORY_BATCH_SIZE,
        _ => DEFAULT_SAFE_HISTORY_BATCH_SIZE,
    }
}

fn safe_event_identity_filter(identities: impl IntoIterator<Item = SafeEventIdentity>) -> Condition {
    identities.into_iter().fold(
        Condition::any(),
        |condition, (safe_contract_id, block, tx_index, log_index)| {
            condition.add(
                Condition::all()
                    .add(hopr_safe_event::Column::HoprSafeContractId.eq(safe_contract_id))
                    .add(hopr_safe_event::Column::PublishedBlock.eq(block))
                    .add(hopr_safe_event::Column::PublishedTxIndex.eq(tx_index))
                    .add(hopr_safe_event::Column::PublishedLogIndex.eq(log_index)),
            )
        },
    )
}

fn safe_event_identity(prepared: &PreparedSafeMutation) -> SafeEventIdentity {
    (
        prepared.safe_contract_id,
        prepared.block,
        prepared.tx_index,
        prepared.log_index,
    )
}

fn ignore_record_not_inserted<T>(result: std::result::Result<T, DbErr>) -> Result<()> {
    match result {
        Ok(_) | Err(DbErr::RecordNotInserted) => Ok(()),
        Err(err) => Err(err.into()),
    }
}

async fn get_safe_contract<C: ConnectionTrait>(
    conn: &C,
    safe_address: Address,
) -> Result<Option<hopr_safe_contract::Model>> {
    HoprSafeContract::find()
        .filter(hopr_safe_contract::Column::Address.eq(safe_address.as_ref().to_vec()))
        .one(conn)
        .await
        .map_err(Into::into)
}

async fn prepare_staged_safe_mutations<C: ConnectionTrait>(
    conn: &C,
    mutations: &[StagedSafeMutation],
) -> Result<Vec<PreparedSafeMutation>> {
    if mutations.is_empty() {
        return Ok(Vec::new());
    }

    let safe_addresses = mutations
        .iter()
        .map(|mutation| mutation.safe_address.as_ref().to_vec())
        .collect::<HashSet<_>>();

    let safe_contracts = HoprSafeContract::find()
        .filter(hopr_safe_contract::Column::Address.is_in(safe_addresses.iter().cloned()))
        .all(conn)
        .await?;

    let safe_contracts_by_address = safe_contracts
        .into_iter()
        .map(|safe| (safe.address.clone(), safe))
        .collect::<HashMap<_, _>>();

    let mut identities = HashSet::with_capacity(mutations.len());
    let mut prepared = Vec::with_capacity(mutations.len());

    for mutation in mutations {
        let Some(safe) = safe_contracts_by_address.get(mutation.safe_address.as_ref()) else {
            return Err(DbSqlError::EntityNotFound(format!(
                "safe contract not found for {}",
                mutation.safe_address.to_hex()
            )));
        };

        let prepared_mutation = PreparedSafeMutation {
            safe_address: mutation.safe_address,
            safe_contract_id: safe.id,
            chain_tx_hash: mutation.chain_tx_hash,
            block: mutation.block.cast_signed(),
            tx_index: mutation.tx_index.cast_signed(),
            log_index: mutation.log_index.cast_signed(),
            event: mutation.event.clone(),
        };

        let identity = safe_event_identity(&prepared_mutation);
        if !identities.insert(identity) {
            return Err(DbSqlError::Construction(format!(
                "duplicate staged safe mutation for {} at {}:{}:{}",
                prepared_mutation.safe_address.to_hex(),
                mutation.block,
                mutation.tx_index,
                mutation.log_index
            )));
        }

        prepared.push(prepared_mutation);
    }

    Ok(prepared)
}

async fn reconcile_safe_event_ids<C: ConnectionTrait>(
    conn: &C,
    prepared_mutations: &[PreparedSafeMutation],
    batch_size: usize,
) -> Result<HashMap<SafeEventIdentity, i64>> {
    let identities = prepared_mutations.iter().map(safe_event_identity).collect::<Vec<_>>();

    let mut event_ids_by_identity = HashMap::with_capacity(identities.len());
    for chunk in identities.chunks(batch_size) {
        let events = HoprSafeEvent::find()
            .filter(safe_event_identity_filter(chunk.iter().copied()))
            .all(conn)
            .await?;

        for event in events {
            event_ids_by_identity.insert(
                (
                    event.hopr_safe_contract_id,
                    event.published_block,
                    event.published_tx_index,
                    event.published_log_index,
                ),
                event.id,
            );
        }
    }

    if identities
        .iter()
        .any(|identity| !event_ids_by_identity.contains_key(identity))
    {
        return Err(DbSqlError::EntityNotFound(
            "failed to reconcile stored safe event ids after bulk insert".to_string(),
        ));
    }

    Ok(event_ids_by_identity)
}

async fn persist_staged_safe_mutations_in<C: ConnectionTrait>(
    conn: &C,
    mutations: Vec<StagedSafeMutation>,
) -> Result<()> {
    if mutations.is_empty() {
        return Ok(());
    }

    let prepared_mutations = prepare_staged_safe_mutations(conn, &mutations).await?;
    let batch_size = safe_history_batch_size(conn.get_database_backend());

    let event_models = prepared_mutations
        .iter()
        .map(|prepared| {
            let event_kind = match &prepared.event {
                StagedSafeMutationKind::SafeSetup { .. } => SafeActivityKind::SafeSetup,
                StagedSafeMutationKind::AddedOwner { .. } => SafeActivityKind::AddedOwner,
                StagedSafeMutationKind::RemovedOwner { .. } => SafeActivityKind::RemovedOwner,
                StagedSafeMutationKind::ChangedThreshold { .. } => SafeActivityKind::ChangedThreshold,
                StagedSafeMutationKind::ExecutionSuccess { .. } => SafeActivityKind::ExecutionSuccess,
                StagedSafeMutationKind::ExecutionFailure { .. } => SafeActivityKind::ExecutionFailure,
            };

            hopr_safe_event::ActiveModel {
                hopr_safe_contract_id: Set(prepared.safe_contract_id),
                event_kind: Set(event_kind.as_str().to_string()),
                chain_tx_hash: Set(prepared.chain_tx_hash.as_ref().to_vec()),
                published_block: Set(prepared.block),
                published_tx_index: Set(prepared.tx_index),
                published_log_index: Set(prepared.log_index),
                ..Default::default()
            }
        })
        .collect::<Vec<_>>();

    for chunk in event_models.chunks(batch_size) {
        HoprSafeEvent::insert_many(chunk.iter().cloned())
            .on_conflict(
                OnConflict::columns([
                    hopr_safe_event::Column::HoprSafeContractId,
                    hopr_safe_event::Column::PublishedBlock,
                    hopr_safe_event::Column::PublishedTxIndex,
                    hopr_safe_event::Column::PublishedLogIndex,
                ])
                .do_nothing()
                .to_owned(),
            )
            .exec_without_returning(conn)
            .await?;
    }

    let event_ids_by_identity = reconcile_safe_event_ids(conn, &prepared_mutations, batch_size).await?;

    let mut setup_event_models = Vec::new();
    let mut setup_owner_models = Vec::new();
    let mut owner_change_models = Vec::new();
    let mut threshold_change_models = Vec::new();
    let mut execution_event_models = Vec::new();
    let mut owner_state_models = Vec::new();
    let mut threshold_state_models = Vec::new();

    for prepared in &prepared_mutations {
        let identity = safe_event_identity(prepared);
        let Some(event_id) = event_ids_by_identity.get(&identity).copied() else {
            return Err(DbSqlError::EntityNotFound(format!(
                "safe event not found after bulk insert for {} at {}:{}:{}",
                prepared.safe_address.to_hex(),
                prepared.block,
                prepared.tx_index,
                prepared.log_index
            )));
        };

        match &prepared.event {
            StagedSafeMutationKind::SafeSetup {
                owners,
                threshold,
                initiator_address,
            } => {
                setup_event_models.push(hopr_safe_setup_event::ActiveModel {
                    hopr_safe_event_id: Set(event_id),
                    initiator_address: Set(initiator_address.map(|address| address.as_ref().to_vec())),
                    threshold: Set(threshold.clone()),
                });

                threshold_state_models.push(hopr_safe_threshold_state::ActiveModel {
                    hopr_safe_contract_id: Set(prepared.safe_contract_id),
                    threshold: Set(threshold.clone()),
                    published_block: Set(prepared.block),
                    published_tx_index: Set(prepared.tx_index),
                    published_log_index: Set(prepared.log_index),
                    ..Default::default()
                });

                for (owner_position, owner_address) in owners.iter().copied().enumerate() {
                    let owner_position = i64::try_from(owner_position).map_err(|_| {
                        DbSqlError::Construction(format!(
                            "safe setup owner position overflow for {}",
                            prepared.safe_address.to_hex()
                        ))
                    })?;

                    setup_owner_models.push(hopr_safe_setup_owner::ActiveModel {
                        hopr_safe_event_id: Set(event_id),
                        owner_position: Set(owner_position),
                        owner_address: Set(owner_address.as_ref().to_vec()),
                        ..Default::default()
                    });

                    owner_state_models.push(hopr_safe_owner_state::ActiveModel {
                        hopr_safe_contract_id: Set(prepared.safe_contract_id),
                        owner_address: Set(owner_address.as_ref().to_vec()),
                        is_current_owner: Set(true),
                        published_block: Set(prepared.block),
                        published_tx_index: Set(prepared.tx_index),
                        published_log_index: Set(prepared.log_index),
                        ..Default::default()
                    });
                }
            }
            StagedSafeMutationKind::AddedOwner { owner_address } => {
                owner_change_models.push(hopr_safe_owner_change_event::ActiveModel {
                    hopr_safe_event_id: Set(event_id),
                    owner_address: Set(owner_address.as_ref().to_vec()),
                });
                owner_state_models.push(hopr_safe_owner_state::ActiveModel {
                    hopr_safe_contract_id: Set(prepared.safe_contract_id),
                    owner_address: Set(owner_address.as_ref().to_vec()),
                    is_current_owner: Set(true),
                    published_block: Set(prepared.block),
                    published_tx_index: Set(prepared.tx_index),
                    published_log_index: Set(prepared.log_index),
                    ..Default::default()
                });
            }
            StagedSafeMutationKind::RemovedOwner { owner_address } => {
                owner_change_models.push(hopr_safe_owner_change_event::ActiveModel {
                    hopr_safe_event_id: Set(event_id),
                    owner_address: Set(owner_address.as_ref().to_vec()),
                });
                owner_state_models.push(hopr_safe_owner_state::ActiveModel {
                    hopr_safe_contract_id: Set(prepared.safe_contract_id),
                    owner_address: Set(owner_address.as_ref().to_vec()),
                    is_current_owner: Set(false),
                    published_block: Set(prepared.block),
                    published_tx_index: Set(prepared.tx_index),
                    published_log_index: Set(prepared.log_index),
                    ..Default::default()
                });
            }
            StagedSafeMutationKind::ChangedThreshold { threshold } => {
                threshold_change_models.push(hopr_safe_threshold_change_event::ActiveModel {
                    hopr_safe_event_id: Set(event_id),
                    threshold: Set(threshold.clone()),
                });
                threshold_state_models.push(hopr_safe_threshold_state::ActiveModel {
                    hopr_safe_contract_id: Set(prepared.safe_contract_id),
                    threshold: Set(threshold.clone()),
                    published_block: Set(prepared.block),
                    published_tx_index: Set(prepared.tx_index),
                    published_log_index: Set(prepared.log_index),
                    ..Default::default()
                });
            }
            StagedSafeMutationKind::ExecutionSuccess { safe_tx_hash, payment }
            | StagedSafeMutationKind::ExecutionFailure { safe_tx_hash, payment } => {
                execution_event_models.push(hopr_safe_execution_event::ActiveModel {
                    hopr_safe_event_id: Set(event_id),
                    safe_tx_hash: Set(safe_tx_hash.as_ref().to_vec()),
                    payment: Set(payment.clone()),
                });
            }
        }
    }

    for chunk in setup_event_models.chunks(batch_size) {
        HoprSafeSetupEvent::insert_many(chunk.iter().cloned())
            .on_conflict(
                OnConflict::column(hopr_safe_setup_event::Column::HoprSafeEventId)
                    .do_nothing()
                    .to_owned(),
            )
            .exec_without_returning(conn)
            .await?;
    }

    for chunk in setup_owner_models.chunks(batch_size) {
        HoprSafeSetupOwner::insert_many(chunk.iter().cloned())
            .on_conflict(
                OnConflict::columns([
                    hopr_safe_setup_owner::Column::HoprSafeEventId,
                    hopr_safe_setup_owner::Column::OwnerPosition,
                ])
                .do_nothing()
                .to_owned(),
            )
            .exec_without_returning(conn)
            .await?;
    }

    for chunk in owner_change_models.chunks(batch_size) {
        HoprSafeOwnerChangeEvent::insert_many(chunk.iter().cloned())
            .on_conflict(
                OnConflict::column(hopr_safe_owner_change_event::Column::HoprSafeEventId)
                    .do_nothing()
                    .to_owned(),
            )
            .exec_without_returning(conn)
            .await?;
    }

    for chunk in threshold_change_models.chunks(batch_size) {
        HoprSafeThresholdChangeEvent::insert_many(chunk.iter().cloned())
            .on_conflict(
                OnConflict::column(hopr_safe_threshold_change_event::Column::HoprSafeEventId)
                    .do_nothing()
                    .to_owned(),
            )
            .exec_without_returning(conn)
            .await?;
    }

    for chunk in execution_event_models.chunks(batch_size) {
        HoprSafeExecutionEvent::insert_many(chunk.iter().cloned())
            .on_conflict(
                OnConflict::column(hopr_safe_execution_event::Column::HoprSafeEventId)
                    .do_nothing()
                    .to_owned(),
            )
            .exec_without_returning(conn)
            .await?;
    }

    for chunk in owner_state_models.chunks(batch_size) {
        HoprSafeOwnerState::insert_many(chunk.iter().cloned())
            .on_conflict(
                OnConflict::columns([
                    hopr_safe_owner_state::Column::HoprSafeContractId,
                    hopr_safe_owner_state::Column::OwnerAddress,
                    hopr_safe_owner_state::Column::PublishedBlock,
                    hopr_safe_owner_state::Column::PublishedTxIndex,
                    hopr_safe_owner_state::Column::PublishedLogIndex,
                ])
                .do_nothing()
                .to_owned(),
            )
            .exec_without_returning(conn)
            .await?;
    }

    for chunk in threshold_state_models.chunks(batch_size) {
        HoprSafeThresholdState::insert_many(chunk.iter().cloned())
            .on_conflict(
                OnConflict::columns([
                    hopr_safe_threshold_state::Column::HoprSafeContractId,
                    hopr_safe_threshold_state::Column::PublishedBlock,
                    hopr_safe_threshold_state::Column::PublishedTxIndex,
                    hopr_safe_threshold_state::Column::PublishedLogIndex,
                ])
                .do_nothing()
                .to_owned(),
            )
            .exec_without_returning(conn)
            .await?;
    }

    Ok(())
}

async fn insert_safe_event<C: ConnectionTrait>(
    conn: &C,
    safe_contract_id: i64,
    event_kind: SafeActivityKind,
    chain_tx_hash: Hash,
    block: u64,
    tx_index: u64,
    log_index: u64,
) -> Result<i64> {
    match HoprSafeEvent::insert(hopr_safe_event::ActiveModel {
        hopr_safe_contract_id: Set(safe_contract_id),
        event_kind: Set(event_kind.as_str().to_string()),
        chain_tx_hash: Set(chain_tx_hash.as_ref().to_vec()),
        published_block: Set(block.cast_signed()),
        published_tx_index: Set(tx_index.cast_signed()),
        published_log_index: Set(log_index.cast_signed()),
        ..Default::default()
    })
    .on_conflict(
        OnConflict::columns([
            hopr_safe_event::Column::HoprSafeContractId,
            hopr_safe_event::Column::PublishedBlock,
            hopr_safe_event::Column::PublishedTxIndex,
            hopr_safe_event::Column::PublishedLogIndex,
        ])
        .do_nothing()
        .to_owned(),
    )
    .exec(conn)
    .await
    {
        Ok(result) => Ok(result.last_insert_id),
        Err(DbErr::RecordNotInserted) => HoprSafeEvent::find()
            .filter(hopr_safe_event::Column::HoprSafeContractId.eq(safe_contract_id))
            .filter(hopr_safe_event::Column::PublishedBlock.eq(block.cast_signed()))
            .filter(hopr_safe_event::Column::PublishedTxIndex.eq(tx_index.cast_signed()))
            .filter(hopr_safe_event::Column::PublishedLogIndex.eq(log_index.cast_signed()))
            .one(conn)
            .await?
            .map(|event| event.id)
            .ok_or_else(|| {
                DbSqlError::EntityNotFound(format!(
                    "safe event not found after insert for contract {safe_contract_id} at \
                     {block}:{tx_index}:{log_index}"
                ))
            }),
        Err(err) => Err(err.into()),
    }
}

async fn load_safe_activity_entries<C: ConnectionTrait>(
    conn: &C,
    safe: &hopr_safe_contract::Model,
) -> Result<Vec<SafeActivityEntry>> {
    let event_rows = HoprSafeEvent::find()
        .filter(hopr_safe_event::Column::HoprSafeContractId.eq(safe.id))
        .order_by_asc(hopr_safe_event::Column::PublishedBlock)
        .order_by_asc(hopr_safe_event::Column::PublishedTxIndex)
        .order_by_asc(hopr_safe_event::Column::PublishedLogIndex)
        .all(conn)
        .await?;

    let headers = event_rows
        .into_iter()
        .map(|event| {
            Ok(SafeEventHeader {
                event_id: event.id,
                event_kind: SafeActivityKind::from_str(&event.event_kind)?,
                chain_tx_hash: event.chain_tx_hash,
                published_block: event.published_block,
                published_tx_index: event.published_tx_index,
                published_log_index: event.published_log_index,
            })
        })
        .collect::<Result<Vec<_>>>()?;

    let mut setup_ids = Vec::new();
    let mut owner_change_ids = Vec::new();
    let mut threshold_change_ids = Vec::new();
    let mut execution_ids = Vec::new();

    for header in &headers {
        match header.event_kind {
            SafeActivityKind::SafeSetup => setup_ids.push(header.event_id),
            SafeActivityKind::AddedOwner | SafeActivityKind::RemovedOwner => owner_change_ids.push(header.event_id),
            SafeActivityKind::ChangedThreshold => threshold_change_ids.push(header.event_id),
            SafeActivityKind::ExecutionSuccess | SafeActivityKind::ExecutionFailure => {
                execution_ids.push(header.event_id)
            }
        }
    }

    let mut setup_details = HashMap::new();
    if !setup_ids.is_empty() {
        for row in HoprSafeSetupEvent::find()
            .filter(hopr_safe_setup_event::Column::HoprSafeEventId.is_in(setup_ids.clone()))
            .all(conn)
            .await?
        {
            setup_details.insert(
                row.hopr_safe_event_id,
                SafeSetupDetail {
                    initiator_address: row.initiator_address,
                    threshold: row.threshold,
                },
            );
        }
    }

    let mut setup_owners: HashMap<i64, Vec<Vec<u8>>> = HashMap::new();
    if !setup_ids.is_empty() {
        for row in HoprSafeSetupOwner::find()
            .filter(hopr_safe_setup_owner::Column::HoprSafeEventId.is_in(setup_ids))
            .order_by_asc(hopr_safe_setup_owner::Column::HoprSafeEventId)
            .order_by_asc(hopr_safe_setup_owner::Column::OwnerPosition)
            .all(conn)
            .await?
        {
            setup_owners
                .entry(row.hopr_safe_event_id)
                .or_default()
                .push(row.owner_address);
        }
    }

    let mut owner_change_details = HashMap::new();
    if !owner_change_ids.is_empty() {
        for row in HoprSafeOwnerChangeEvent::find()
            .filter(hopr_safe_owner_change_event::Column::HoprSafeEventId.is_in(owner_change_ids))
            .all(conn)
            .await?
        {
            owner_change_details.insert(
                row.hopr_safe_event_id,
                SafeOwnerChangeDetail {
                    owner_address: row.owner_address,
                },
            );
        }
    }

    let mut threshold_change_details = HashMap::new();
    if !threshold_change_ids.is_empty() {
        for row in HoprSafeThresholdChangeEvent::find()
            .filter(hopr_safe_threshold_change_event::Column::HoprSafeEventId.is_in(threshold_change_ids))
            .all(conn)
            .await?
        {
            threshold_change_details.insert(
                row.hopr_safe_event_id,
                SafeThresholdChangeDetail {
                    threshold: row.threshold,
                },
            );
        }
    }

    let mut execution_details = HashMap::new();
    if !execution_ids.is_empty() {
        for row in HoprSafeExecutionEvent::find()
            .filter(hopr_safe_execution_event::Column::HoprSafeEventId.is_in(execution_ids))
            .all(conn)
            .await?
        {
            execution_details.insert(
                row.hopr_safe_event_id,
                SafeExecutionDetail {
                    safe_tx_hash: row.safe_tx_hash,
                    payment: row.payment,
                },
            );
        }
    }

    headers
        .into_iter()
        .map(|header| {
            let mut entry = SafeActivityEntry {
                safe_address: safe.address.clone(),
                event_kind: header.event_kind.as_str().to_string(),
                chain_tx_hash: header.chain_tx_hash,
                safe_tx_hash: None,
                owner_address: None,
                owners: Vec::new(),
                threshold: None,
                payment: None,
                initiator_address: None,
                published_block: header.published_block,
                published_tx_index: header.published_tx_index,
                published_log_index: header.published_log_index,
            };

            match header.event_kind {
                SafeActivityKind::SafeSetup => {
                    let detail = setup_details.get(&header.event_id).ok_or_else(|| {
                        DbSqlError::EntityNotFound(format!("safe setup detail not found for event {}", header.event_id))
                    })?;
                    entry.initiator_address = detail.initiator_address.clone();
                    entry.threshold = Some(detail.threshold.clone());
                    entry.owners = setup_owners.remove(&header.event_id).unwrap_or_default();
                }
                SafeActivityKind::AddedOwner | SafeActivityKind::RemovedOwner => {
                    let detail = owner_change_details.get(&header.event_id).ok_or_else(|| {
                        DbSqlError::EntityNotFound(format!(
                            "safe owner change detail not found for event {}",
                            header.event_id
                        ))
                    })?;
                    entry.owner_address = Some(detail.owner_address.clone());
                }
                SafeActivityKind::ChangedThreshold => {
                    let detail = threshold_change_details.get(&header.event_id).ok_or_else(|| {
                        DbSqlError::EntityNotFound(format!(
                            "safe threshold detail not found for event {}",
                            header.event_id
                        ))
                    })?;
                    entry.threshold = Some(detail.threshold.clone());
                }
                SafeActivityKind::ExecutionSuccess | SafeActivityKind::ExecutionFailure => {
                    let detail = execution_details.get(&header.event_id).ok_or_else(|| {
                        DbSqlError::EntityNotFound(format!(
                            "safe execution detail not found for event {}",
                            header.event_id
                        ))
                    })?;
                    entry.safe_tx_hash = Some(detail.safe_tx_hash.clone());
                    entry.payment = Some(detail.payment.clone());
                }
            }
            Ok(entry)
        })
        .collect()
}

#[allow(clippy::too_many_arguments)]
async fn record_safe_setup_in<C: ConnectionTrait>(
    conn: &C,
    safe_address: Address,
    chain_tx_hash: Hash,
    owners: Vec<Address>,
    threshold: String,
    initiator_address: Option<Address>,
    block: u64,
    tx_index: u64,
    log_index: u64,
) -> Result<i64> {
    let safe = get_safe_contract(conn, safe_address)
        .await?
        .ok_or_else(|| DbSqlError::EntityNotFound(format!("safe contract not found for {}", safe_address.to_hex())))?;
    let event_id = insert_safe_event(
        conn,
        safe.id,
        SafeActivityKind::SafeSetup,
        chain_tx_hash,
        block,
        tx_index,
        log_index,
    )
    .await?;

    ignore_record_not_inserted(
        HoprSafeSetupEvent::insert(hopr_safe_setup_event::ActiveModel {
            hopr_safe_event_id: Set(event_id),
            initiator_address: Set(initiator_address.map(|address| address.as_ref().to_vec())),
            threshold: Set(threshold.clone()),
        })
        .on_conflict(
            OnConflict::column(hopr_safe_setup_event::Column::HoprSafeEventId)
                .do_nothing()
                .to_owned(),
        )
        .exec(conn)
        .await,
    )?;

    for (owner_position, owner_address) in owners.iter().copied().enumerate() {
        let owner_position = i64::try_from(owner_position).map_err(|_| {
            DbSqlError::Construction(format!(
                "safe setup owner position overflow for {}",
                safe_address.to_hex()
            ))
        })?;
        ignore_record_not_inserted(
            HoprSafeSetupOwner::insert(hopr_safe_setup_owner::ActiveModel {
                hopr_safe_event_id: Set(event_id),
                owner_position: Set(owner_position),
                owner_address: Set(owner_address.as_ref().to_vec()),
                ..Default::default()
            })
            .on_conflict(
                OnConflict::columns([
                    hopr_safe_setup_owner::Column::HoprSafeEventId,
                    hopr_safe_setup_owner::Column::OwnerPosition,
                ])
                .do_nothing()
                .to_owned(),
            )
            .exec(conn)
            .await,
        )?;
    }

    ignore_record_not_inserted(
        HoprSafeThresholdState::insert(hopr_safe_threshold_state::ActiveModel {
            hopr_safe_contract_id: Set(safe.id),
            threshold: Set(threshold),
            published_block: Set(block.cast_signed()),
            published_tx_index: Set(tx_index.cast_signed()),
            published_log_index: Set(log_index.cast_signed()),
            ..Default::default()
        })
        .on_conflict(
            OnConflict::columns([
                hopr_safe_threshold_state::Column::HoprSafeContractId,
                hopr_safe_threshold_state::Column::PublishedBlock,
                hopr_safe_threshold_state::Column::PublishedTxIndex,
                hopr_safe_threshold_state::Column::PublishedLogIndex,
            ])
            .do_nothing()
            .to_owned(),
        )
        .exec(conn)
        .await,
    )?;

    Ok(event_id)
}

#[allow(clippy::too_many_arguments)]
async fn record_safe_activity_in<C: ConnectionTrait>(
    conn: &C,
    safe_address: Address,
    event_kind: SafeActivityKind,
    chain_tx_hash: Hash,
    safe_tx_hash: Option<Hash>,
    owner_address: Option<Address>,
    threshold: Option<String>,
    payment: Option<String>,
    block: u64,
    tx_index: u64,
    log_index: u64,
) -> Result<i64> {
    let safe = get_safe_contract(conn, safe_address)
        .await?
        .ok_or_else(|| DbSqlError::EntityNotFound(format!("safe contract not found for {}", safe_address.to_hex())))?;
    let event_id = insert_safe_event(conn, safe.id, event_kind, chain_tx_hash, block, tx_index, log_index).await?;

    match event_kind {
        SafeActivityKind::SafeSetup => {
            return Err(DbSqlError::Construction(
                "record_safe_setup must be used for SafeSetup events".to_string(),
            ));
        }
        SafeActivityKind::AddedOwner | SafeActivityKind::RemovedOwner => {
            let owner_address = owner_address
                .ok_or_else(|| DbSqlError::Construction(format!("{} requires owner_address", event_kind.as_str())))?;
            ignore_record_not_inserted(
                HoprSafeOwnerChangeEvent::insert(hopr_safe_owner_change_event::ActiveModel {
                    hopr_safe_event_id: Set(event_id),
                    owner_address: Set(owner_address.as_ref().to_vec()),
                })
                .on_conflict(
                    OnConflict::column(hopr_safe_owner_change_event::Column::HoprSafeEventId)
                        .do_nothing()
                        .to_owned(),
                )
                .exec(conn)
                .await,
            )?;
        }
        SafeActivityKind::ChangedThreshold => {
            let threshold = threshold
                .ok_or_else(|| DbSqlError::Construction("CHANGED_THRESHOLD requires threshold".to_string()))?;
            ignore_record_not_inserted(
                HoprSafeThresholdChangeEvent::insert(hopr_safe_threshold_change_event::ActiveModel {
                    hopr_safe_event_id: Set(event_id),
                    threshold: Set(threshold.clone()),
                })
                .on_conflict(
                    OnConflict::column(hopr_safe_threshold_change_event::Column::HoprSafeEventId)
                        .do_nothing()
                        .to_owned(),
                )
                .exec(conn)
                .await,
            )?;
            ignore_record_not_inserted(
                HoprSafeThresholdState::insert(hopr_safe_threshold_state::ActiveModel {
                    hopr_safe_contract_id: Set(safe.id),
                    threshold: Set(threshold),
                    published_block: Set(block.cast_signed()),
                    published_tx_index: Set(tx_index.cast_signed()),
                    published_log_index: Set(log_index.cast_signed()),
                    ..Default::default()
                })
                .on_conflict(
                    OnConflict::columns([
                        hopr_safe_threshold_state::Column::HoprSafeContractId,
                        hopr_safe_threshold_state::Column::PublishedBlock,
                        hopr_safe_threshold_state::Column::PublishedTxIndex,
                        hopr_safe_threshold_state::Column::PublishedLogIndex,
                    ])
                    .do_nothing()
                    .to_owned(),
                )
                .exec(conn)
                .await,
            )?;
        }
        SafeActivityKind::ExecutionSuccess | SafeActivityKind::ExecutionFailure => {
            let safe_tx_hash = safe_tx_hash
                .ok_or_else(|| DbSqlError::Construction(format!("{} requires safe_tx_hash", event_kind.as_str())))?;
            let payment =
                payment.ok_or_else(|| DbSqlError::Construction(format!("{} requires payment", event_kind.as_str())))?;
            ignore_record_not_inserted(
                HoprSafeExecutionEvent::insert(hopr_safe_execution_event::ActiveModel {
                    hopr_safe_event_id: Set(event_id),
                    safe_tx_hash: Set(safe_tx_hash.as_ref().to_vec()),
                    payment: Set(payment),
                })
                .on_conflict(
                    OnConflict::column(hopr_safe_execution_event::Column::HoprSafeEventId)
                        .do_nothing()
                        .to_owned(),
                )
                .exec(conn)
                .await,
            )?;
        }
    }

    Ok(event_id)
}

#[allow(clippy::too_many_arguments)]
async fn upsert_safe_owner_state_in<C: ConnectionTrait>(
    conn: &C,
    safe_address: Address,
    owner_address: Address,
    is_current_owner: bool,
    block: u64,
    tx_index: u64,
    log_index: u64,
) -> Result<i64> {
    let safe = get_safe_contract(conn, safe_address)
        .await?
        .ok_or_else(|| DbSqlError::EntityNotFound(format!("safe contract not found for {}", safe_address.to_hex())))?;
    match HoprSafeOwnerState::insert(hopr_safe_owner_state::ActiveModel {
        hopr_safe_contract_id: Set(safe.id),
        owner_address: Set(owner_address.as_ref().to_vec()),
        is_current_owner: Set(is_current_owner),
        published_block: Set(block.cast_signed()),
        published_tx_index: Set(tx_index.cast_signed()),
        published_log_index: Set(log_index.cast_signed()),
        ..Default::default()
    })
    .on_conflict(
        OnConflict::columns([
            hopr_safe_owner_state::Column::HoprSafeContractId,
            hopr_safe_owner_state::Column::OwnerAddress,
            hopr_safe_owner_state::Column::PublishedBlock,
            hopr_safe_owner_state::Column::PublishedTxIndex,
            hopr_safe_owner_state::Column::PublishedLogIndex,
        ])
        .do_nothing()
        .to_owned(),
    )
    .exec(conn)
    .await
    {
        Ok(result) => Ok(result.last_insert_id),
        Err(DbErr::RecordNotInserted) => HoprSafeOwnerState::find()
            .filter(hopr_safe_owner_state::Column::HoprSafeContractId.eq(safe.id))
            .filter(hopr_safe_owner_state::Column::OwnerAddress.eq(owner_address.as_ref().to_vec()))
            .filter(hopr_safe_owner_state::Column::PublishedBlock.eq(block.cast_signed()))
            .filter(hopr_safe_owner_state::Column::PublishedTxIndex.eq(tx_index.cast_signed()))
            .filter(hopr_safe_owner_state::Column::PublishedLogIndex.eq(log_index.cast_signed()))
            .one(conn)
            .await?
            .map(|state| state.id)
            .ok_or_else(|| {
                DbSqlError::EntityNotFound(format!(
                    "safe owner state not found after insert for {} owner {} at {}:{}:{}",
                    safe_address.to_hex(),
                    owner_address.to_hex(),
                    block,
                    tx_index,
                    log_index
                ))
            }),
        Err(err) => Err(err.into()),
    }
}

#[async_trait]
pub trait BlokliDbSafeHistoryOperations: BlokliDbGeneralModelOperations + BlokliDbSafeContractOperations {
    async fn persist_staged_safe_mutations<'a>(
        &'a self,
        tx: OptTx<'a>,
        mutations: Vec<StagedSafeMutation>,
    ) -> Result<()>;

    #[allow(clippy::too_many_arguments)]
    async fn record_safe_setup<'a>(
        &'a self,
        tx: OptTx<'a>,
        safe_address: Address,
        chain_tx_hash: Hash,
        owners: Vec<Address>,
        threshold: String,
        initiator_address: Option<Address>,
        block: u64,
        tx_index: u64,
        log_index: u64,
    ) -> Result<i64>;

    #[allow(clippy::too_many_arguments)]
    async fn record_safe_activity<'a>(
        &'a self,
        tx: OptTx<'a>,
        safe_address: Address,
        event_kind: SafeActivityKind,
        chain_tx_hash: Hash,
        safe_tx_hash: Option<Hash>,
        owner_address: Option<Address>,
        threshold: Option<String>,
        payment: Option<String>,
        initiator_address: Option<Address>,
        block: u64,
        tx_index: u64,
        log_index: u64,
    ) -> Result<i64>;

    #[allow(clippy::too_many_arguments)]
    async fn upsert_safe_owner_state<'a>(
        &'a self,
        tx: OptTx<'a>,
        safe_address: Address,
        owner_address: Address,
        is_current_owner: bool,
        block: u64,
        tx_index: u64,
        log_index: u64,
    ) -> Result<i64>;

    async fn get_safe_owners<'a>(&'a self, tx: OptTx<'a>, safe_address: Address) -> Result<Vec<Address>>;

    async fn get_safe_activity<'a>(&'a self, tx: OptTx<'a>, safe_address: Address) -> Result<Vec<SafeActivityEntry>>;
}

#[async_trait]
impl BlokliDbSafeHistoryOperations for BlokliDb {
    async fn persist_staged_safe_mutations<'a>(
        &'a self,
        tx: OptTx<'a>,
        mutations: Vec<StagedSafeMutation>,
    ) -> Result<()> {
        with_opt_transaction(self, tx, TargetDb::Index, |tx| {
            Box::pin(async move { persist_staged_safe_mutations_in(tx.as_ref(), mutations).await })
        })
        .await
    }

    #[allow(clippy::too_many_arguments)]
    async fn record_safe_setup<'a>(
        &'a self,
        tx: OptTx<'a>,
        safe_address: Address,
        chain_tx_hash: Hash,
        owners: Vec<Address>,
        threshold: String,
        initiator_address: Option<Address>,
        block: u64,
        tx_index: u64,
        log_index: u64,
    ) -> Result<i64> {
        with_opt_transaction(self, tx, TargetDb::Index, |tx| {
            Box::pin(async move {
                record_safe_setup_in(
                    tx.as_ref(),
                    safe_address,
                    chain_tx_hash,
                    owners,
                    threshold,
                    initiator_address,
                    block,
                    tx_index,
                    log_index,
                )
                .await
            })
        })
        .await
    }

    #[allow(clippy::too_many_arguments)]
    async fn record_safe_activity<'a>(
        &'a self,
        tx: OptTx<'a>,
        safe_address: Address,
        event_kind: SafeActivityKind,
        chain_tx_hash: Hash,
        safe_tx_hash: Option<Hash>,
        owner_address: Option<Address>,
        threshold: Option<String>,
        payment: Option<String>,
        initiator_address: Option<Address>,
        block: u64,
        tx_index: u64,
        log_index: u64,
    ) -> Result<i64> {
        if initiator_address.is_some() {
            return Err(DbSqlError::Construction(
                "initiator_address is only supported for SafeSetup events".to_string(),
            ));
        }

        with_opt_transaction(self, tx, TargetDb::Index, |tx| {
            Box::pin(async move {
                record_safe_activity_in(
                    tx.as_ref(),
                    safe_address,
                    event_kind,
                    chain_tx_hash,
                    safe_tx_hash,
                    owner_address,
                    threshold,
                    payment,
                    block,
                    tx_index,
                    log_index,
                )
                .await
            })
        })
        .await
    }

    #[allow(clippy::too_many_arguments)]
    async fn upsert_safe_owner_state<'a>(
        &'a self,
        tx: OptTx<'a>,
        safe_address: Address,
        owner_address: Address,
        is_current_owner: bool,
        block: u64,
        tx_index: u64,
        log_index: u64,
    ) -> Result<i64> {
        with_opt_transaction(self, tx, TargetDb::Index, |tx| {
            Box::pin(async move {
                upsert_safe_owner_state_in(
                    tx.as_ref(),
                    safe_address,
                    owner_address,
                    is_current_owner,
                    block,
                    tx_index,
                    log_index,
                )
                .await
            })
        })
        .await
    }

    async fn get_safe_owners<'a>(&'a self, tx: OptTx<'a>, safe_address: Address) -> Result<Vec<Address>> {
        let nested = self.nest_transaction(tx).await?;
        let Some(safe) = get_safe_contract(nested.as_ref(), safe_address).await? else {
            nested.commit().await?;
            return Ok(Vec::new());
        };

        let states = HoprSafeOwnerState::find()
            .filter(hopr_safe_owner_state::Column::HoprSafeContractId.eq(safe.id))
            .order_by_desc(hopr_safe_owner_state::Column::PublishedBlock)
            .order_by_desc(hopr_safe_owner_state::Column::PublishedTxIndex)
            .order_by_desc(hopr_safe_owner_state::Column::PublishedLogIndex)
            .all(nested.as_ref())
            .await?;

        let mut seen = HashSet::new();
        let mut owner_bytes = Vec::new();
        for state in states {
            if seen.insert(state.owner_address.clone()) && state.is_current_owner {
                owner_bytes.push(state.owner_address);
            }
        }

        owner_bytes.sort();
        let owners = owner_bytes
            .into_iter()
            .map(|owner| {
                Address::try_from(owner.as_slice())
                    .map_err(|e| DbSqlError::Construction(format!("invalid safe owner address length: {e}")))
            })
            .collect::<Result<Vec<_>>>()?;

        nested.commit().await?;
        Ok(owners)
    }

    async fn get_safe_activity<'a>(&'a self, tx: OptTx<'a>, safe_address: Address) -> Result<Vec<SafeActivityEntry>> {
        let nested = self.nest_transaction(tx).await?;
        let Some(safe) = get_safe_contract(nested.as_ref(), safe_address).await? else {
            nested.commit().await?;
            return Ok(Vec::new());
        };
        let entries = load_safe_activity_entries(nested.as_ref(), &safe).await?;
        nested.commit().await?;
        Ok(entries)
    }
}

#[cfg(test)]
mod tests {
    use blokli_db_entity::prelude::{
        HoprSafeEvent, HoprSafeExecutionEvent, HoprSafeOwnerChangeEvent, HoprSafeOwnerState, HoprSafeSetupEvent,
        HoprSafeSetupOwner, HoprSafeThresholdChangeEvent, HoprSafeThresholdState,
    };
    use sea_orm::PaginatorTrait;

    use super::*;
    use crate::{db::BlokliDb, safe_contracts::BlokliDbSafeContractOperations};

    fn random_address() -> Address {
        Address::from(hopr_types::crypto_random::random_bytes())
    }

    fn random_hash() -> Hash {
        Hash::from(hopr_types::crypto_random::random_bytes())
    }

    fn staged_safe_mutations(
        safe_address: Address,
        owner_one: Address,
        owner_two: Address,
        owner_three: Address,
        initiator: Address,
        chain_tx_hashes: [Hash; 5],
        safe_tx_hash: Hash,
    ) -> Vec<StagedSafeMutation> {
        vec![
            StagedSafeMutation {
                safe_address,
                chain_tx_hash: chain_tx_hashes[0],
                block: 101,
                tx_index: 0,
                log_index: 0,
                event: StagedSafeMutationKind::SafeSetup {
                    owners: vec![owner_one, owner_two],
                    threshold: "2".to_string(),
                    initiator_address: Some(initiator),
                },
            },
            StagedSafeMutation {
                safe_address,
                chain_tx_hash: chain_tx_hashes[1],
                block: 102,
                tx_index: 0,
                log_index: 0,
                event: StagedSafeMutationKind::AddedOwner {
                    owner_address: owner_three,
                },
            },
            StagedSafeMutation {
                safe_address,
                chain_tx_hash: chain_tx_hashes[2],
                block: 103,
                tx_index: 0,
                log_index: 0,
                event: StagedSafeMutationKind::ChangedThreshold {
                    threshold: "3".to_string(),
                },
            },
            StagedSafeMutation {
                safe_address,
                chain_tx_hash: chain_tx_hashes[3],
                block: 104,
                tx_index: 0,
                log_index: 0,
                event: StagedSafeMutationKind::ExecutionSuccess {
                    safe_tx_hash,
                    payment: "12".to_string(),
                },
            },
            StagedSafeMutation {
                safe_address,
                chain_tx_hash: chain_tx_hashes[4],
                block: 105,
                tx_index: 0,
                log_index: 0,
                event: StagedSafeMutationKind::RemovedOwner {
                    owner_address: owner_two,
                },
            },
        ]
    }

    #[tokio::test]
    async fn test_get_safe_owners_tracks_current_membership() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;

        let safe_address = random_address();
        let module_address = random_address();
        let chain_key = random_address();
        let owner_one: Address = "1111111111111111111111111111111111111111".parse()?;
        let owner_two: Address = "2222222222222222222222222222222222222222".parse()?;

        db.create_safe_contract(None, safe_address, module_address, chain_key, 100, 0, 0)
            .await?;

        let first_id = db
            .upsert_safe_owner_state(None, safe_address, owner_one, true, 101, 0, 0)
            .await?;
        let duplicate_id = db
            .upsert_safe_owner_state(None, safe_address, owner_one, true, 101, 0, 0)
            .await?;
        assert_eq!(
            first_id, duplicate_id,
            "duplicate owner state insert should be idempotent"
        );

        db.upsert_safe_owner_state(None, safe_address, owner_two, true, 102, 0, 0)
            .await?;
        db.upsert_safe_owner_state(None, safe_address, owner_one, false, 103, 0, 0)
            .await?;

        let current_owners = db.get_safe_owners(None, safe_address).await?;
        assert_eq!(current_owners, vec![owner_two], "only owner two should still be active");

        db.upsert_safe_owner_state(None, safe_address, owner_one, true, 104, 0, 0)
            .await?;

        let current_owners = db.get_safe_owners(None, safe_address).await?;
        assert_eq!(
            current_owners,
            vec![owner_one, owner_two],
            "re-adding owner one should restore both current owners in sorted order"
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_get_safe_activity_is_ordered_and_idempotent() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;

        let safe_address = random_address();
        let module_address = random_address();
        let chain_key = random_address();
        let owner = random_address();
        let owner_two = random_address();
        let chain_tx_hash = random_hash();
        let safe_tx_hash = random_hash();

        db.create_safe_contract(None, safe_address, module_address, chain_key, 100, 0, 0)
            .await?;

        let first_id = db
            .record_safe_setup(
                None,
                safe_address,
                random_hash(),
                vec![owner, owner_two],
                "2".to_string(),
                Some(chain_key),
                109,
                1,
                0,
            )
            .await?;
        let duplicate_id = db
            .record_safe_setup(
                None,
                safe_address,
                random_hash(),
                vec![owner, owner_two],
                "2".to_string(),
                Some(chain_key),
                109,
                1,
                0,
            )
            .await?;
        assert_eq!(first_id, duplicate_id, "duplicate setup insert should be idempotent");

        db.record_safe_activity(
            None,
            safe_address,
            SafeActivityKind::AddedOwner,
            chain_tx_hash,
            None,
            Some(owner),
            None,
            None,
            None,
            110,
            2,
            0,
        )
        .await?;

        db.record_safe_activity(
            None,
            safe_address,
            SafeActivityKind::ExecutionSuccess,
            random_hash(),
            Some(safe_tx_hash),
            None,
            None,
            Some("12".to_string()),
            None,
            111,
            0,
            1,
        )
        .await?;

        let activity = db.get_safe_activity(None, safe_address).await?;
        assert_eq!(activity.len(), 3);
        assert_eq!(activity[0].event_kind, SafeActivityKind::SafeSetup.as_str());
        assert_eq!(activity[0].threshold.as_deref(), Some("2"));
        assert_eq!(activity[0].initiator_address, Some(chain_key.as_ref().to_vec()));
        assert_eq!(
            activity[0].owners,
            vec![owner.as_ref().to_vec(), owner_two.as_ref().to_vec()]
        );
        assert_eq!(activity[1].event_kind, SafeActivityKind::AddedOwner.as_str());
        assert_eq!(activity[1].owner_address, Some(owner.as_ref().to_vec()));
        assert_eq!(activity[1].published_block, 110);
        assert_eq!(activity[2].event_kind, SafeActivityKind::ExecutionSuccess.as_str());
        assert_eq!(activity[2].safe_tx_hash, Some(safe_tx_hash.as_ref().to_vec()));
        assert_eq!(activity[2].payment.as_deref(), Some("12"));
        assert_eq!(activity[2].published_block, 111);

        Ok(())
    }

    #[tokio::test]
    async fn test_persist_staged_safe_mutations_is_idempotent() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;

        let safe_address = random_address();
        let module_address = random_address();
        let chain_key = random_address();
        let owner_one = random_address();
        let owner_two = random_address();
        let owner_three = random_address();
        let chain_tx_hashes = [
            random_hash(),
            random_hash(),
            random_hash(),
            random_hash(),
            random_hash(),
        ];
        let safe_tx_hash = random_hash();

        db.create_safe_contract(None, safe_address, module_address, chain_key, 100, 0, 0)
            .await?;

        let mutations = staged_safe_mutations(
            safe_address,
            owner_one,
            owner_two,
            owner_three,
            chain_key,
            chain_tx_hashes,
            safe_tx_hash,
        );

        db.persist_staged_safe_mutations(None, mutations.clone()).await?;
        db.persist_staged_safe_mutations(None, mutations).await?;

        let owners = db.get_safe_owners(None, safe_address).await?;
        let mut expected_owners = vec![owner_one, owner_three];
        expected_owners.sort_unstable();
        assert_eq!(owners, expected_owners);

        let activity = db.get_safe_activity(None, safe_address).await?;
        assert_eq!(activity.len(), 5);
        assert_eq!(activity[0].event_kind, SafeActivityKind::SafeSetup.as_str());
        assert_eq!(activity[1].event_kind, SafeActivityKind::AddedOwner.as_str());
        assert_eq!(activity[2].event_kind, SafeActivityKind::ChangedThreshold.as_str());
        assert_eq!(activity[3].event_kind, SafeActivityKind::ExecutionSuccess.as_str());
        assert_eq!(activity[4].event_kind, SafeActivityKind::RemovedOwner.as_str());

        assert_eq!(HoprSafeEvent::find().count(db.conn(TargetDb::Index)).await?, 5);
        assert_eq!(HoprSafeSetupEvent::find().count(db.conn(TargetDb::Index)).await?, 1);
        assert_eq!(HoprSafeSetupOwner::find().count(db.conn(TargetDb::Index)).await?, 2);
        assert_eq!(
            HoprSafeOwnerChangeEvent::find().count(db.conn(TargetDb::Index)).await?,
            2
        );
        assert_eq!(
            HoprSafeThresholdChangeEvent::find()
                .count(db.conn(TargetDb::Index))
                .await?,
            1
        );
        assert_eq!(HoprSafeExecutionEvent::find().count(db.conn(TargetDb::Index)).await?, 1);
        assert_eq!(HoprSafeOwnerState::find().count(db.conn(TargetDb::Index)).await?, 4);
        assert_eq!(HoprSafeThresholdState::find().count(db.conn(TargetDb::Index)).await?, 2);

        Ok(())
    }

    #[tokio::test]
    async fn test_persist_staged_safe_mutations_matches_single_event_path() -> anyhow::Result<()> {
        let bulk_db = BlokliDb::new_in_memory().await?;
        let single_db = BlokliDb::new_in_memory().await?;

        let safe_address = random_address();
        let module_address = random_address();
        let chain_key = random_address();
        let owner_one = random_address();
        let owner_two = random_address();
        let owner_three = random_address();
        let chain_tx_hashes = [
            random_hash(),
            random_hash(),
            random_hash(),
            random_hash(),
            random_hash(),
        ];
        let safe_tx_hash = random_hash();

        for db in [&bulk_db, &single_db] {
            db.create_safe_contract(None, safe_address, module_address, chain_key, 100, 0, 0)
                .await?;
        }

        let mutations = staged_safe_mutations(
            safe_address,
            owner_one,
            owner_two,
            owner_three,
            chain_key,
            chain_tx_hashes,
            safe_tx_hash,
        );

        bulk_db.persist_staged_safe_mutations(None, mutations.clone()).await?;

        single_db
            .record_safe_setup(
                None,
                safe_address,
                chain_tx_hashes[0],
                vec![owner_one, owner_two],
                "2".to_string(),
                Some(chain_key),
                101,
                0,
                0,
            )
            .await?;
        single_db
            .upsert_safe_owner_state(None, safe_address, owner_one, true, 101, 0, 0)
            .await?;
        single_db
            .upsert_safe_owner_state(None, safe_address, owner_two, true, 101, 0, 0)
            .await?;
        single_db
            .record_safe_activity(
                None,
                safe_address,
                SafeActivityKind::AddedOwner,
                chain_tx_hashes[1],
                None,
                Some(owner_three),
                None,
                None,
                None,
                102,
                0,
                0,
            )
            .await?;
        single_db
            .upsert_safe_owner_state(None, safe_address, owner_three, true, 102, 0, 0)
            .await?;
        single_db
            .record_safe_activity(
                None,
                safe_address,
                SafeActivityKind::ChangedThreshold,
                chain_tx_hashes[2],
                None,
                None,
                Some("3".to_string()),
                None,
                None,
                103,
                0,
                0,
            )
            .await?;
        single_db
            .record_safe_activity(
                None,
                safe_address,
                SafeActivityKind::ExecutionSuccess,
                chain_tx_hashes[3],
                Some(safe_tx_hash),
                None,
                None,
                Some("12".to_string()),
                None,
                104,
                0,
                0,
            )
            .await?;
        single_db
            .record_safe_activity(
                None,
                safe_address,
                SafeActivityKind::RemovedOwner,
                chain_tx_hashes[4],
                None,
                Some(owner_two),
                None,
                None,
                None,
                105,
                0,
                0,
            )
            .await?;
        single_db
            .upsert_safe_owner_state(None, safe_address, owner_two, false, 105, 0, 0)
            .await?;

        let bulk_activity = bulk_db.get_safe_activity(None, safe_address).await?;
        let single_activity = single_db.get_safe_activity(None, safe_address).await?;
        assert_eq!(bulk_activity, single_activity);

        let bulk_owners = bulk_db.get_safe_owners(None, safe_address).await?;
        let single_owners = single_db.get_safe_owners(None, safe_address).await?;
        assert_eq!(bulk_owners, single_owners);

        Ok(())
    }
}
