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
use sea_orm::{ColumnTrait, ConnectionTrait, DbErr, EntityTrait, QueryFilter, QueryOrder, Set};
use sea_query::OnConflict;

use crate::{
    BlokliDb, BlokliDbGeneralModelOperations, DbSqlError, OptTx, Result, safe_contracts::BlokliDbSafeContractOperations,
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

#[async_trait]
pub trait BlokliDbSafeHistoryOperations: BlokliDbGeneralModelOperations + BlokliDbSafeContractOperations {
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
        let tx = self.nest_transaction(tx).await?;
        let safe = get_safe_contract(tx.as_ref(), safe_address).await?.ok_or_else(|| {
            DbSqlError::EntityNotFound(format!("safe contract not found for {}", safe_address.to_hex()))
        })?;
        let event_id = insert_safe_event(
            tx.as_ref(),
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
            .exec(tx.as_ref())
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
                .exec(tx.as_ref())
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
            .exec(tx.as_ref())
            .await,
        )?;

        tx.commit().await?;
        Ok(event_id)
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

        let tx = self.nest_transaction(tx).await?;
        let safe = get_safe_contract(tx.as_ref(), safe_address).await?.ok_or_else(|| {
            DbSqlError::EntityNotFound(format!("safe contract not found for {}", safe_address.to_hex()))
        })?;
        let event_id = insert_safe_event(
            tx.as_ref(),
            safe.id,
            event_kind,
            chain_tx_hash,
            block,
            tx_index,
            log_index,
        )
        .await?;

        match event_kind {
            SafeActivityKind::SafeSetup => {
                return Err(DbSqlError::Construction(
                    "record_safe_setup must be used for SafeSetup events".to_string(),
                ));
            }
            SafeActivityKind::AddedOwner | SafeActivityKind::RemovedOwner => {
                let owner_address = owner_address.ok_or_else(|| {
                    DbSqlError::Construction(format!("{} requires owner_address", event_kind.as_str()))
                })?;
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
                    .exec(tx.as_ref())
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
                    .exec(tx.as_ref())
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
                    .exec(tx.as_ref())
                    .await,
                )?;
            }
            SafeActivityKind::ExecutionSuccess | SafeActivityKind::ExecutionFailure => {
                let safe_tx_hash = safe_tx_hash.ok_or_else(|| {
                    DbSqlError::Construction(format!("{} requires safe_tx_hash", event_kind.as_str()))
                })?;
                let payment = payment
                    .ok_or_else(|| DbSqlError::Construction(format!("{} requires payment", event_kind.as_str())))?;
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
                    .exec(tx.as_ref())
                    .await,
                )?;
            }
        }

        tx.commit().await?;
        Ok(event_id)
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
        let tx = self.nest_transaction(tx).await?;
        let safe = get_safe_contract(tx.as_ref(), safe_address).await?.ok_or_else(|| {
            DbSqlError::EntityNotFound(format!("safe contract not found for {}", safe_address.to_hex()))
        })?;
        let id = match HoprSafeOwnerState::insert(hopr_safe_owner_state::ActiveModel {
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
        .exec(tx.as_ref())
        .await
        {
            Ok(result) => result.last_insert_id,
            Err(DbErr::RecordNotInserted) => HoprSafeOwnerState::find()
                .filter(hopr_safe_owner_state::Column::HoprSafeContractId.eq(safe.id))
                .filter(hopr_safe_owner_state::Column::OwnerAddress.eq(owner_address.as_ref().to_vec()))
                .filter(hopr_safe_owner_state::Column::PublishedBlock.eq(block.cast_signed()))
                .filter(hopr_safe_owner_state::Column::PublishedTxIndex.eq(tx_index.cast_signed()))
                .filter(hopr_safe_owner_state::Column::PublishedLogIndex.eq(log_index.cast_signed()))
                .one(tx.as_ref())
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
                })?,
            Err(err) => return Err(err.into()),
        };

        tx.commit().await?;
        Ok(id)
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
    use super::*;
    use crate::{db::BlokliDb, safe_contracts::BlokliDbSafeContractOperations};

    fn random_address() -> Address {
        Address::from(hopr_types::crypto_random::random_bytes())
    }

    fn random_hash() -> Hash {
        Hash::from(hopr_types::crypto_random::random_bytes())
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
}
