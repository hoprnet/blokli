//! Database operations for `HoprServiceRegistry`, the permissionless registry of the services
//! HOPR nodes offer.
//!
//! The registry is stored with the same temporal shape as accounts and channels: `service_type`
//! and `service_entry` are identity tables that never change, `service_type_state` and
//! `service_entry_state` are append-only and keyed by the log position that produced them, and the
//! `service_type_current` / `service_entry_current` views project the newest row per identity.
//!
//! Every write appends a state row with `ON CONFLICT DO NOTHING` on the unique position index, so
//! replaying an already indexed log is a no-op rather than a duplicate row or an error.

use std::time::SystemTime;

use async_trait::async_trait;
use blokli_db_entity::{
    prelude::{
        ServiceEntry as ServiceEntryEntity, ServiceEntryState, ServiceRegistryConfig, ServiceType as ServiceTypeEntity,
        ServiceTypeState,
    },
    service_entry, service_entry_state, service_registry_config, service_type, service_type_state,
    views::{service_entry_current, service_type_current},
};
use hopr_types::{
    internal::prelude::{ServiceEntry, ServiceMetadata, ServiceType},
    primitive::prelude::{Address, DateTime, HoprBalance, IntoEndian, Utc},
};
use sea_orm::{ColumnTrait, ConnectionTrait, DbErr, EntityTrait, QueryFilter, Set};
use sea_query::OnConflict;

use crate::{
    BlokliDb, BlokliDbGeneralModelOperations, DbSqlError, OptTx, Result, SINGULAR_TABLE_FIXED_ID,
    numeric::log_position_to_i64,
};

/// Number of bytes a burn or fee amount occupies in the database.
const BALANCE_BYTES: usize = 32;

/// The current state of a service type, as the registry reports it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ServiceTypeInfo {
    /// The service type this state belongs to.
    pub service_type: ServiceType,
    /// Owner of the type, or `None` when the type was abandoned.
    pub owner: Option<Address>,
    /// Requirement contract gating the type, or `None` when the type is open to any node.
    pub requirement: Option<Address>,
    /// Amount a node burns to register an entry under this type.
    pub registration_burn: HoprBalance,
    /// Amount a node burns to update an entry under this type.
    pub update_burn: HoprBalance,
}

/// The registry-wide configuration values, which belong to no single service type.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ServiceRegistryConfigInfo {
    /// Amount an operator burns to register a new service type.
    pub type_registration_fee: HoprBalance,
    /// The `NodeSafeRegistry` the registry checks node-safe bindings against, or `None` until the
    /// registry has reported it.
    pub node_safe_registry: Option<Address>,
}

#[async_trait]
pub trait BlokliDbServiceOperations: BlokliDbGeneralModelOperations {
    /// Record the current content of a service registry entry.
    ///
    /// Both the `Registered` and the `Updated` registry events carry a complete entry, so both map
    /// to this single append: a new `service_entry_state` row at the given log position. The
    /// identity rows for the service type and for the entry are created on demand, because the
    /// registry allows an entry for a node that never announced itself and this method must not
    /// depend on the service type log having been indexed first.
    ///
    /// # Arguments
    /// * `entry` - The complete entry as the registry reported it
    /// * `block` - Event block number
    /// * `tx_index` - Event transaction index
    /// * `log_index` - Event log index
    ///
    /// # Returns
    /// The database id of the `service_entry` identity row.
    ///
    /// # Idempotency
    /// Uses ON CONFLICT DO NOTHING on the unique position index, so replaying the same log is a
    /// no-op.
    async fn upsert_service_entry<'a>(
        &'a self,
        tx: OptTx<'a>,
        entry: &ServiceEntry,
        block: u64,
        tx_index: u64,
        log_index: u64,
    ) -> Result<i64>;

    /// Record that an entry was removed from the registry.
    ///
    /// Appends a tombstone state row rather than deleting history: the entry columns are all
    /// `NULL` and `deregistered` is set. A later `Registered` event revives the same identity row.
    ///
    /// # Returns
    /// * `Ok(())` - The tombstone was appended, or the log had already been indexed
    /// * `Err(DbSqlError::EntityNotFound)` - No entry exists for this service type and node
    #[allow(clippy::too_many_arguments)]
    async fn deregister_service_entry<'a>(
        &'a self,
        tx: OptTx<'a>,
        service_type: ServiceType,
        node: Address,
        block: u64,
        tx_index: u64,
        log_index: u64,
    ) -> Result<()>;

    /// Get the live entry a node published under a service type.
    ///
    /// Returns `None` when the node never registered under the type, and also when its entry was
    /// deregistered.
    async fn get_service_entry<'a>(
        &'a self,
        tx: OptTx<'a>,
        service_type: ServiceType,
        node: Address,
    ) -> Result<Option<ServiceEntry>>;

    /// Get every live entry published under a service type.
    async fn get_service_entries_for_type<'a>(
        &'a self,
        tx: OptTx<'a>,
        service_type: ServiceType,
    ) -> Result<Vec<ServiceEntry>>;

    /// Get every live entry a node published, across all service types.
    async fn get_service_entries_for_node<'a>(&'a self, tx: OptTx<'a>, node: Address) -> Result<Vec<ServiceEntry>>;

    /// Record the registration of a new service type.
    ///
    /// Creates the identity row and its first state row: owned by `owner`, open to any node, with
    /// both burns at zero until the owner sets them.
    ///
    /// # Returns
    /// The database id of the `service_type` identity row.
    #[allow(clippy::too_many_arguments)]
    async fn register_service_type<'a>(
        &'a self,
        tx: OptTx<'a>,
        service_type: ServiceType,
        owner: Address,
        block: u64,
        tx_index: u64,
        log_index: u64,
    ) -> Result<i64>;

    /// Record a change of the owner of a service type.
    ///
    /// `None` marks the type as abandoned, which is how the registry encodes a transfer to the
    /// zero address.
    #[allow(clippy::too_many_arguments)]
    async fn set_service_type_owner<'a>(
        &'a self,
        tx: OptTx<'a>,
        service_type: ServiceType,
        owner: Option<Address>,
        block: u64,
        tx_index: u64,
        log_index: u64,
    ) -> Result<()>;

    /// Record a change of the requirement contract gating a service type.
    ///
    /// `None` reopens the type to any node.
    #[allow(clippy::too_many_arguments)]
    async fn set_service_type_requirement<'a>(
        &'a self,
        tx: OptTx<'a>,
        service_type: ServiceType,
        requirement: Option<Address>,
        block: u64,
        tx_index: u64,
        log_index: u64,
    ) -> Result<()>;

    /// Record a change of the amount a node burns to register under a service type.
    #[allow(clippy::too_many_arguments)]
    async fn set_service_type_registration_burn<'a>(
        &'a self,
        tx: OptTx<'a>,
        service_type: ServiceType,
        burn: HoprBalance,
        block: u64,
        tx_index: u64,
        log_index: u64,
    ) -> Result<()>;

    /// Record a change of the amount a node burns to update an entry under a service type.
    #[allow(clippy::too_many_arguments)]
    async fn set_service_type_update_burn<'a>(
        &'a self,
        tx: OptTx<'a>,
        service_type: ServiceType,
        burn: HoprBalance,
        block: u64,
        tx_index: u64,
        log_index: u64,
    ) -> Result<()>;

    /// Get the current state of a single service type.
    async fn get_service_type<'a>(
        &'a self,
        tx: OptTx<'a>,
        service_type: ServiceType,
    ) -> Result<Option<ServiceTypeInfo>>;

    /// Get the current state of every service type the registry knows.
    async fn get_service_types<'a>(&'a self, tx: OptTx<'a>) -> Result<Vec<ServiceTypeInfo>>;

    /// Record a change of the fee an operator burns to register a new service type.
    ///
    /// A log at a position older than the one already applied is ignored, so replaying config
    /// events cannot resurrect a stale value.
    async fn set_type_registration_fee<'a>(
        &'a self,
        tx: OptTx<'a>,
        fee: HoprBalance,
        block: u64,
        tx_index: u64,
        log_index: u64,
    ) -> Result<()>;

    /// Record a change of the `NodeSafeRegistry` the registry checks node-safe bindings against.
    ///
    /// A log at a position older than the one already applied is ignored.
    async fn set_node_safe_registry<'a>(
        &'a self,
        tx: OptTx<'a>,
        registry: Address,
        block: u64,
        tx_index: u64,
        log_index: u64,
    ) -> Result<()>;

    /// Get the registry-wide configuration values.
    async fn get_service_registry_config<'a>(&'a self, tx: OptTx<'a>) -> Result<ServiceRegistryConfigInfo>;
}

/// The single field a registry event changes on a service type.
///
/// Each variant carries the new value; every other field is copied forward from the current state.
enum ServiceTypeChange {
    Owner(Option<Address>),
    Requirement(Option<Address>),
    RegistrationBurn(HoprBalance),
    UpdateBurn(HoprBalance),
}

fn address_to_bytes(address: Address) -> Vec<u8> {
    address.as_ref().to_vec()
}

fn balance_to_bytes(balance: HoprBalance) -> Vec<u8> {
    balance.to_be_bytes().to_vec()
}

fn datetime_to_db(time: SystemTime) -> sea_orm::prelude::DateTimeWithTimeZone {
    DateTime::<Utc>::from(time).into()
}

fn db_to_datetime(time: sea_orm::prelude::DateTimeWithTimeZone) -> SystemTime {
    time.with_timezone(&Utc).into()
}

/// Reports a live entry row that is missing a column only a deregistration tombstone may omit.
fn missing_column(service_entry_id: i64, column: &str) -> DbSqlError {
    DbSqlError::InvalidData(format!("service entry {service_entry_id} is live but has no {column}"))
}

/// Converts a `service_entry_current` row into the domain entry.
///
/// Returns `Ok(None)` for a deregistration tombstone, which is not an entry any more.
fn view_to_service_entry(row: service_entry_current::Model) -> Result<Option<ServiceEntry>> {
    if row.deregistered {
        return Ok(None);
    }

    let service_type = ServiceType::try_from(row.service_type.as_slice())?;
    let node = Address::try_from(row.node_address.as_slice())?;
    let safe_bytes = row
        .safe_address
        .as_deref()
        .ok_or_else(|| missing_column(row.service_entry_id, "safe address"))?;
    let safe = Address::try_from(safe_bytes)?;
    let metadata = ServiceMetadata::try_from(
        row.metadata
            .clone()
            .ok_or_else(|| missing_column(row.service_entry_id, "metadata"))?,
    )?;
    let registered_at = row
        .registered_at
        .ok_or_else(|| missing_column(row.service_entry_id, "registration timestamp"))?;
    let updated_at = row
        .updated_at
        .ok_or_else(|| missing_column(row.service_entry_id, "update timestamp"))?;

    Ok(Some(ServiceEntry::new(
        service_type,
        node,
        safe,
        metadata,
        db_to_datetime(registered_at),
        db_to_datetime(updated_at),
    )?))
}

/// Converts a `service_type_current` row into the domain state.
fn view_to_service_type_info(row: service_type_current::Model) -> Result<ServiceTypeInfo> {
    let owner = row
        .owner_address
        .as_deref()
        .map(Address::try_from)
        .transpose()
        .map_err(DbSqlError::from)?;
    let requirement = row
        .requirement_address
        .as_deref()
        .map(Address::try_from)
        .transpose()
        .map_err(DbSqlError::from)?;

    Ok(ServiceTypeInfo {
        service_type: ServiceType::try_from(row.service_type.as_slice())?,
        owner,
        requirement,
        registration_burn: HoprBalance::from_be_bytes(row.registration_burn.as_slice()),
        update_burn: HoprBalance::from_be_bytes(row.update_burn.as_slice()),
    })
}

/// Treats `RecordNotInserted` as success: it is what `ON CONFLICT DO NOTHING` reports when the log
/// behind this row has already been indexed.
fn ignore_replay<T>(result: std::result::Result<T, DbErr>) -> Result<()> {
    match result {
        Ok(_) | Err(DbErr::RecordNotInserted) => Ok(()),
        Err(e) => Err(e.into()),
    }
}

/// Returns the id of the identity row of `service_type`, creating it when absent.
async fn get_or_create_service_type_id<C: ConnectionTrait>(conn: &C, service_type: ServiceType) -> Result<i64> {
    ignore_replay(
        ServiceTypeEntity::insert(service_type::ActiveModel {
            service_type: Set(service_type.as_ref().to_vec()),
            ..Default::default()
        })
        .on_conflict(
            OnConflict::column(service_type::Column::ServiceType)
                .do_nothing()
                .to_owned(),
        )
        .exec(conn)
        .await,
    )?;

    find_service_type_id(conn, service_type)
        .await?
        .ok_or_else(|| DbSqlError::EntityNotFound(format!("service type {service_type} not found after insert")))
}

/// Returns the id of the identity row of `service_type`, or `None` when the type is unknown.
async fn find_service_type_id<C: ConnectionTrait>(conn: &C, service_type: ServiceType) -> Result<Option<i64>> {
    Ok(ServiceTypeEntity::find()
        .filter(service_type::Column::ServiceType.eq(service_type.as_ref().to_vec()))
        .one(conn)
        .await?
        .map(|model| model.id))
}

/// Returns the id of the identity row of the `(service type, node)` pair, creating it when absent.
async fn get_or_create_service_entry_id<C: ConnectionTrait>(
    conn: &C,
    service_type_id: i64,
    node: Address,
) -> Result<i64> {
    ignore_replay(
        ServiceEntryEntity::insert(service_entry::ActiveModel {
            service_type_id: Set(service_type_id),
            node_address: Set(address_to_bytes(node)),
            ..Default::default()
        })
        .on_conflict(
            OnConflict::columns([service_entry::Column::ServiceTypeId, service_entry::Column::NodeAddress])
                .do_nothing()
                .to_owned(),
        )
        .exec(conn)
        .await,
    )?;

    find_service_entry_id(conn, service_type_id, node)
        .await?
        .ok_or_else(|| DbSqlError::EntityNotFound(format!("service entry for node {node} not found after insert")))
}

/// Returns the id of the identity row of the `(service type, node)` pair, or `None` when the pair
/// was never registered.
async fn find_service_entry_id<C: ConnectionTrait>(
    conn: &C,
    service_type_id: i64,
    node: Address,
) -> Result<Option<i64>> {
    Ok(ServiceEntryEntity::find()
        .filter(service_entry::Column::ServiceTypeId.eq(service_type_id))
        .filter(service_entry::Column::NodeAddress.eq(address_to_bytes(node)))
        .one(conn)
        .await?
        .map(|model| model.id))
}

/// Appends a service entry state row at the given log position.
async fn append_service_entry_state<C: ConnectionTrait>(
    conn: &C,
    state: service_entry_state::ActiveModel,
) -> Result<()> {
    ignore_replay(
        ServiceEntryState::insert(state)
            .on_conflict(
                OnConflict::columns([
                    service_entry_state::Column::ServiceEntryId,
                    service_entry_state::Column::PublishedBlock,
                    service_entry_state::Column::PublishedTxIndex,
                    service_entry_state::Column::PublishedLogIndex,
                ])
                .do_nothing()
                .to_owned(),
            )
            .exec(conn)
            .await,
    )
}

/// Appends a service type state row that carries `change` and copies every other field forward
/// from the current state.
///
/// The base is the state the `service_type_current` view reports, which is the state immediately
/// preceding this log as long as logs are indexed in order.
async fn append_service_type_state<C: ConnectionTrait>(
    conn: &C,
    service_type_id: i64,
    change: ServiceTypeChange,
    block: i64,
    tx_index: i64,
    log_index: i64,
) -> Result<()> {
    let current = service_type_current::Entity::find()
        .filter(service_type_current::Column::ServiceTypeId.eq(service_type_id))
        .one(conn)
        .await?;

    let mut next = service_type_state::ActiveModel {
        service_type_id: Set(service_type_id),
        owner_address: Set(current.as_ref().and_then(|s| s.owner_address.clone())),
        requirement_address: Set(current.as_ref().and_then(|s| s.requirement_address.clone())),
        registration_burn: Set(current
            .as_ref()
            .map_or_else(|| vec![0u8; BALANCE_BYTES], |s| s.registration_burn.clone())),
        update_burn: Set(current
            .as_ref()
            .map_or_else(|| vec![0u8; BALANCE_BYTES], |s| s.update_burn.clone())),
        published_block: Set(block),
        published_tx_index: Set(tx_index),
        published_log_index: Set(log_index),
        ..Default::default()
    };

    match change {
        ServiceTypeChange::Owner(owner) => next.owner_address = Set(owner.map(address_to_bytes)),
        ServiceTypeChange::Requirement(requirement) => {
            next.requirement_address = Set(requirement.map(address_to_bytes))
        }
        ServiceTypeChange::RegistrationBurn(burn) => next.registration_burn = Set(balance_to_bytes(burn)),
        ServiceTypeChange::UpdateBurn(burn) => next.update_burn = Set(balance_to_bytes(burn)),
    }

    ignore_replay(
        ServiceTypeState::insert(next)
            .on_conflict(
                OnConflict::columns([
                    service_type_state::Column::ServiceTypeId,
                    service_type_state::Column::PublishedBlock,
                    service_type_state::Column::PublishedTxIndex,
                    service_type_state::Column::PublishedLogIndex,
                ])
                .do_nothing()
                .to_owned(),
            )
            .exec(conn)
            .await,
    )
}

/// Runs `change` against an existing service type, failing when the type is unknown.
async fn apply_service_type_change(
    db: &BlokliDb,
    tx: OptTx<'_>,
    service_type: ServiceType,
    change: ServiceTypeChange,
    block: u64,
    tx_index: u64,
    log_index: u64,
) -> Result<()> {
    let tx = db.nest_transaction(tx).await?;
    let (block, tx_index, log_index) = log_position_to_i64(block, tx_index, log_index)?;

    let service_type_id = find_service_type_id(tx.as_ref(), service_type)
        .await?
        .ok_or_else(|| DbSqlError::EntityNotFound(format!("service type {service_type} is not registered")))?;

    append_service_type_state(tx.as_ref(), service_type_id, change, block, tx_index, log_index).await?;

    tx.commit().await
}

/// Applies a partial update to the registry config singleton, unless a newer log has already been
/// applied to it.
///
/// The singleton carries the position of the newest config log it reflects. Comparing against it
/// makes a replay of an older config log a no-op instead of a stale overwrite.
async fn update_registry_config(
    db: &BlokliDb,
    tx: OptTx<'_>,
    mut update: service_registry_config::ActiveModel,
    block: u64,
    tx_index: u64,
    log_index: u64,
) -> Result<()> {
    let tx = db.nest_transaction(tx).await?;
    let (block, tx_index, log_index) = log_position_to_i64(block, tx_index, log_index)?;

    let current = ServiceRegistryConfig::find_by_id(SINGULAR_TABLE_FIXED_ID)
        .one(tx.as_ref())
        .await?
        .ok_or_else(|| DbSqlError::MissingFixedTableEntry("service_registry_config".to_string()))?;

    let applied = (
        current.last_changed_block,
        current.last_changed_tx_index,
        current.last_changed_log_index,
    );

    if (block, tx_index, log_index) < applied {
        return tx.commit().await;
    }

    update.last_changed_block = Set(block);
    update.last_changed_tx_index = Set(tx_index);
    update.last_changed_log_index = Set(log_index);

    ServiceRegistryConfig::update(update).exec(tx.as_ref()).await?;

    tx.commit().await
}

#[async_trait]
impl BlokliDbServiceOperations for BlokliDb {
    async fn upsert_service_entry<'a>(
        &'a self,
        tx: OptTx<'a>,
        entry: &ServiceEntry,
        block: u64,
        tx_index: u64,
        log_index: u64,
    ) -> Result<i64> {
        let tx = self.nest_transaction(tx).await?;
        let (block, tx_index, log_index) = log_position_to_i64(block, tx_index, log_index)?;

        let service_type_id = get_or_create_service_type_id(tx.as_ref(), entry.service_type).await?;
        let service_entry_id = get_or_create_service_entry_id(tx.as_ref(), service_type_id, entry.node).await?;

        append_service_entry_state(
            tx.as_ref(),
            service_entry_state::ActiveModel {
                service_entry_id: Set(service_entry_id),
                safe_address: Set(Some(address_to_bytes(entry.safe))),
                metadata: Set(Some(entry.metadata.as_ref().to_vec())),
                registered_at: Set(Some(datetime_to_db(entry.registered_at))),
                updated_at: Set(Some(datetime_to_db(entry.updated_at))),
                deregistered: Set(false),
                published_block: Set(block),
                published_tx_index: Set(tx_index),
                published_log_index: Set(log_index),
                ..Default::default()
            },
        )
        .await?;

        tx.commit().await?;
        Ok(service_entry_id)
    }

    async fn deregister_service_entry<'a>(
        &'a self,
        tx: OptTx<'a>,
        service_type: ServiceType,
        node: Address,
        block: u64,
        tx_index: u64,
        log_index: u64,
    ) -> Result<()> {
        let tx = self.nest_transaction(tx).await?;
        let (block, tx_index, log_index) = log_position_to_i64(block, tx_index, log_index)?;

        let service_type_id = find_service_type_id(tx.as_ref(), service_type)
            .await?
            .ok_or_else(|| DbSqlError::EntityNotFound(format!("service type {service_type} is not registered")))?;

        let service_entry_id = find_service_entry_id(tx.as_ref(), service_type_id, node)
            .await?
            .ok_or_else(|| {
                DbSqlError::EntityNotFound(format!("no entry of service type {service_type} for node {node}"))
            })?;

        append_service_entry_state(
            tx.as_ref(),
            service_entry_state::ActiveModel {
                service_entry_id: Set(service_entry_id),
                safe_address: Set(None),
                metadata: Set(None),
                registered_at: Set(None),
                updated_at: Set(None),
                deregistered: Set(true),
                published_block: Set(block),
                published_tx_index: Set(tx_index),
                published_log_index: Set(log_index),
                ..Default::default()
            },
        )
        .await?;

        tx.commit().await
    }

    async fn get_service_entry<'a>(
        &'a self,
        tx: OptTx<'a>,
        service_type: ServiceType,
        node: Address,
    ) -> Result<Option<ServiceEntry>> {
        let query = service_entry_current::Entity::find()
            .filter(service_entry_current::Column::ServiceType.eq(service_type.as_ref().to_vec()))
            .filter(service_entry_current::Column::NodeAddress.eq(address_to_bytes(node)));

        let row = if let Some(t) = tx {
            query.one(t.as_ref()).await?
        } else {
            query.one(self.conn(crate::TargetDb::Index)).await?
        };

        row.map(view_to_service_entry).transpose().map(Option::flatten)
    }

    async fn get_service_entries_for_type<'a>(
        &'a self,
        tx: OptTx<'a>,
        service_type: ServiceType,
    ) -> Result<Vec<ServiceEntry>> {
        let query = service_entry_current::Entity::find()
            .filter(service_entry_current::Column::ServiceType.eq(service_type.as_ref().to_vec()))
            .filter(service_entry_current::Column::Deregistered.eq(false));

        let rows = if let Some(t) = tx {
            query.all(t.as_ref()).await?
        } else {
            query.all(self.conn(crate::TargetDb::Index)).await?
        };

        collect_live_entries(rows)
    }

    async fn get_service_entries_for_node<'a>(&'a self, tx: OptTx<'a>, node: Address) -> Result<Vec<ServiceEntry>> {
        let query = service_entry_current::Entity::find()
            .filter(service_entry_current::Column::NodeAddress.eq(address_to_bytes(node)))
            .filter(service_entry_current::Column::Deregistered.eq(false));

        let rows = if let Some(t) = tx {
            query.all(t.as_ref()).await?
        } else {
            query.all(self.conn(crate::TargetDb::Index)).await?
        };

        collect_live_entries(rows)
    }

    async fn register_service_type<'a>(
        &'a self,
        tx: OptTx<'a>,
        service_type: ServiceType,
        owner: Address,
        block: u64,
        tx_index: u64,
        log_index: u64,
    ) -> Result<i64> {
        let tx = self.nest_transaction(tx).await?;
        let (block, tx_index, log_index) = log_position_to_i64(block, tx_index, log_index)?;

        let service_type_id = get_or_create_service_type_id(tx.as_ref(), service_type).await?;

        append_service_type_state(
            tx.as_ref(),
            service_type_id,
            ServiceTypeChange::Owner(Some(owner)),
            block,
            tx_index,
            log_index,
        )
        .await?;

        tx.commit().await?;
        Ok(service_type_id)
    }

    async fn set_service_type_owner<'a>(
        &'a self,
        tx: OptTx<'a>,
        service_type: ServiceType,
        owner: Option<Address>,
        block: u64,
        tx_index: u64,
        log_index: u64,
    ) -> Result<()> {
        apply_service_type_change(
            self,
            tx,
            service_type,
            ServiceTypeChange::Owner(owner),
            block,
            tx_index,
            log_index,
        )
        .await
    }

    async fn set_service_type_requirement<'a>(
        &'a self,
        tx: OptTx<'a>,
        service_type: ServiceType,
        requirement: Option<Address>,
        block: u64,
        tx_index: u64,
        log_index: u64,
    ) -> Result<()> {
        apply_service_type_change(
            self,
            tx,
            service_type,
            ServiceTypeChange::Requirement(requirement),
            block,
            tx_index,
            log_index,
        )
        .await
    }

    async fn set_service_type_registration_burn<'a>(
        &'a self,
        tx: OptTx<'a>,
        service_type: ServiceType,
        burn: HoprBalance,
        block: u64,
        tx_index: u64,
        log_index: u64,
    ) -> Result<()> {
        apply_service_type_change(
            self,
            tx,
            service_type,
            ServiceTypeChange::RegistrationBurn(burn),
            block,
            tx_index,
            log_index,
        )
        .await
    }

    async fn set_service_type_update_burn<'a>(
        &'a self,
        tx: OptTx<'a>,
        service_type: ServiceType,
        burn: HoprBalance,
        block: u64,
        tx_index: u64,
        log_index: u64,
    ) -> Result<()> {
        apply_service_type_change(
            self,
            tx,
            service_type,
            ServiceTypeChange::UpdateBurn(burn),
            block,
            tx_index,
            log_index,
        )
        .await
    }

    async fn get_service_type<'a>(
        &'a self,
        tx: OptTx<'a>,
        service_type: ServiceType,
    ) -> Result<Option<ServiceTypeInfo>> {
        let query = service_type_current::Entity::find()
            .filter(service_type_current::Column::ServiceType.eq(service_type.as_ref().to_vec()));

        let row = if let Some(t) = tx {
            query.one(t.as_ref()).await?
        } else {
            query.one(self.conn(crate::TargetDb::Index)).await?
        };

        row.map(view_to_service_type_info).transpose()
    }

    async fn get_service_types<'a>(&'a self, tx: OptTx<'a>) -> Result<Vec<ServiceTypeInfo>> {
        let query = service_type_current::Entity::find();

        let rows = if let Some(t) = tx {
            query.all(t.as_ref()).await?
        } else {
            query.all(self.conn(crate::TargetDb::Index)).await?
        };

        rows.into_iter().map(view_to_service_type_info).collect()
    }

    async fn set_type_registration_fee<'a>(
        &'a self,
        tx: OptTx<'a>,
        fee: HoprBalance,
        block: u64,
        tx_index: u64,
        log_index: u64,
    ) -> Result<()> {
        let update = service_registry_config::ActiveModel {
            id: Set(SINGULAR_TABLE_FIXED_ID),
            type_registration_fee: Set(balance_to_bytes(fee)),
            ..Default::default()
        };

        update_registry_config(self, tx, update, block, tx_index, log_index).await
    }

    async fn set_node_safe_registry<'a>(
        &'a self,
        tx: OptTx<'a>,
        registry: Address,
        block: u64,
        tx_index: u64,
        log_index: u64,
    ) -> Result<()> {
        let update = service_registry_config::ActiveModel {
            id: Set(SINGULAR_TABLE_FIXED_ID),
            node_safe_registry: Set(Some(address_to_bytes(registry))),
            ..Default::default()
        };

        update_registry_config(self, tx, update, block, tx_index, log_index).await
    }

    async fn get_service_registry_config<'a>(&'a self, tx: OptTx<'a>) -> Result<ServiceRegistryConfigInfo> {
        let query = ServiceRegistryConfig::find_by_id(SINGULAR_TABLE_FIXED_ID);

        let model = if let Some(t) = tx {
            query.one(t.as_ref()).await?
        } else {
            query.one(self.conn(crate::TargetDb::Index)).await?
        }
        .ok_or_else(|| DbSqlError::MissingFixedTableEntry("service_registry_config".to_string()))?;

        let node_safe_registry = model
            .node_safe_registry
            .as_deref()
            .map(Address::try_from)
            .transpose()
            .map_err(DbSqlError::from)?;

        Ok(ServiceRegistryConfigInfo {
            type_registration_fee: HoprBalance::from_be_bytes(model.type_registration_fee.as_slice()),
            node_safe_registry,
        })
    }
}

/// Drops the deregistration tombstones from a set of view rows and converts the rest.
fn collect_live_entries(rows: Vec<service_entry_current::Model>) -> Result<Vec<ServiceEntry>> {
    let mut entries = Vec::with_capacity(rows.len());
    for row in rows {
        if let Some(entry) = view_to_service_entry(row)? {
            entries.push(entry);
        }
    }
    Ok(entries)
}

#[cfg(test)]
mod tests {
    use std::{
        collections::HashMap,
        time::{Duration, UNIX_EPOCH},
    };

    use blokli_db_entity::{
        account,
        conversions::service_aggregation::{
            fetch_service_entries, fetch_service_entries_for_nodes, fetch_service_entries_for_type, fetch_service_types,
        },
        prelude::Account,
    };
    use hopr_types::{crypto_random::random_bytes, primitive::prelude::ToHex};
    use sea_orm::{ActiveModelTrait, PaginatorTrait};

    use super::*;
    use crate::db::BlokliDb;

    fn random_address() -> Address {
        Address::from(random_bytes())
    }

    /// A whole-second timestamp, so that the value survives the database round trip exactly.
    fn timestamp(offset_secs: u64) -> SystemTime {
        UNIX_EPOCH + Duration::from_secs(1_700_000_000 + offset_secs)
    }

    fn entry(
        service_type: ServiceType,
        node: Address,
        safe: Address,
        metadata: &[u8],
        registered_at: SystemTime,
        updated_at: SystemTime,
    ) -> anyhow::Result<ServiceEntry> {
        Ok(ServiceEntry::new(
            service_type,
            node,
            safe,
            ServiceMetadata::try_from(metadata.to_vec())?,
            registered_at,
            updated_at,
        )?)
    }

    async fn state_row_count(db: &BlokliDb) -> anyhow::Result<u64> {
        Ok(ServiceEntryState::find().count(db.conn(crate::TargetDb::Index)).await?)
    }

    #[tokio::test]
    async fn test_service_entry_register_update_deregister_reregister() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;

        let service_type = ServiceType::GVPN_EXIT;
        let node = random_address();
        let safe = random_address();

        let registered = entry(service_type, node, safe, b"v1", timestamp(0), timestamp(0))?;
        db.upsert_service_entry(None, &registered, 100, 0, 0).await?;
        assert_eq!(db.get_service_entry(None, service_type, node).await?, Some(registered));

        let updated = entry(service_type, node, safe, b"v2", timestamp(0), timestamp(60))?;
        db.upsert_service_entry(None, &updated, 200, 0, 0).await?;
        assert_eq!(db.get_service_entry(None, service_type, node).await?, Some(updated));

        db.deregister_service_entry(None, service_type, node, 300, 0, 0).await?;
        assert_eq!(db.get_service_entry(None, service_type, node).await?, None);
        assert!(db.get_service_entries_for_type(None, service_type).await?.is_empty());
        assert!(db.get_service_entries_for_node(None, node).await?.is_empty());

        let reregistered = entry(service_type, node, safe, b"v3", timestamp(120), timestamp(120))?;
        db.upsert_service_entry(None, &reregistered, 400, 0, 0).await?;
        assert_eq!(
            db.get_service_entry(None, service_type, node).await?,
            Some(reregistered.clone())
        );
        assert_eq!(
            db.get_service_entries_for_node(None, node).await?,
            vec![reregistered.clone()]
        );
        assert_eq!(
            db.get_service_entries_for_type(None, service_type).await?,
            vec![reregistered]
        );

        // Re-registering revives the identity row rather than creating a second one, and every
        // step left its own state row behind.
        let identity_rows = ServiceEntryEntity::find()
            .count(db.conn(crate::TargetDb::Index))
            .await?;
        assert_eq!(identity_rows, 1);
        assert_eq!(state_row_count(&db).await?, 4);

        Ok(())
    }

    /// Replaying logs the indexer has already seen, in any order, must leave the database exactly
    /// as it was: the unique position index rejects the duplicate state rows.
    #[tokio::test]
    async fn test_out_of_order_log_replay_is_idempotent() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;

        let service_type = ServiceType::GVPN_EXIT;
        let node = random_address();
        let safe = random_address();

        let registered = entry(service_type, node, safe, b"v1", timestamp(0), timestamp(0))?;
        let updated = entry(service_type, node, safe, b"v2", timestamp(0), timestamp(60))?;

        db.upsert_service_entry(None, &registered, 100, 0, 0).await?;
        db.upsert_service_entry(None, &updated, 200, 0, 0).await?;
        db.deregister_service_entry(None, service_type, node, 300, 0, 0).await?;
        assert_eq!(state_row_count(&db).await?, 3);

        // Replay the three logs in reverse order.
        db.deregister_service_entry(None, service_type, node, 300, 0, 0).await?;
        db.upsert_service_entry(None, &updated, 200, 0, 0).await?;
        db.upsert_service_entry(None, &registered, 100, 0, 0).await?;

        assert_eq!(state_row_count(&db).await?, 3);
        assert_eq!(
            ServiceEntryEntity::find()
                .count(db.conn(crate::TargetDb::Index))
                .await?,
            1
        );
        // The newest position still wins: the entry stays deregistered.
        assert_eq!(db.get_service_entry(None, service_type, node).await?, None);

        Ok(())
    }

    #[tokio::test]
    async fn test_deregister_unknown_entry_fails() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;

        let service_type = ServiceType::GVPN_EXIT;
        let node = random_address();

        let unknown_type = db.deregister_service_entry(None, service_type, node, 100, 0, 0).await;
        assert!(matches!(unknown_type, Err(DbSqlError::EntityNotFound(_))));

        db.register_service_type(None, service_type, random_address(), 50, 0, 0)
            .await?;

        let unknown_node = db.deregister_service_entry(None, service_type, node, 100, 0, 0).await;
        assert!(matches!(unknown_node, Err(DbSqlError::EntityNotFound(_))));

        Ok(())
    }

    /// A registry entry may exist for a node that never announced itself, so no `account` row is
    /// required for the write to succeed.
    #[tokio::test]
    async fn test_service_entry_needs_no_account_row() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;

        let node = random_address();
        let registered = entry(
            ServiceType::GVPN_EXIT,
            node,
            random_address(),
            b"metadata",
            timestamp(0),
            timestamp(0),
        )?;

        db.upsert_service_entry(None, &registered, 100, 0, 0).await?;

        let accounts = Account::find().count(db.conn(crate::TargetDb::Index)).await?;
        assert_eq!(accounts, 0);
        assert_eq!(
            db.get_service_entry(None, ServiceType::GVPN_EXIT, node).await?,
            Some(registered)
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_service_type_state_transitions() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;

        let service_type = ServiceType::GVPN_EXIT;
        let owner = random_address();
        let requirement = random_address();

        db.register_service_type(None, service_type, owner, 100, 0, 0).await?;
        assert_eq!(
            db.get_service_type(None, service_type).await?,
            Some(ServiceTypeInfo {
                service_type,
                owner: Some(owner),
                requirement: None,
                registration_burn: HoprBalance::from(0),
                update_burn: HoprBalance::from(0),
            })
        );

        db.set_service_type_requirement(None, service_type, Some(requirement), 200, 0, 0)
            .await?;
        db.set_service_type_registration_burn(None, service_type, HoprBalance::from(5), 300, 0, 0)
            .await?;
        db.set_service_type_update_burn(None, service_type, HoprBalance::from(7), 400, 0, 0)
            .await?;

        // Abandoning the type keeps every other field, which is what copying the state forward
        // one field at a time is for.
        db.set_service_type_owner(None, service_type, None, 500, 0, 0).await?;

        let expected = ServiceTypeInfo {
            service_type,
            owner: None,
            requirement: Some(requirement),
            registration_burn: HoprBalance::from(5),
            update_burn: HoprBalance::from(7),
        };
        assert_eq!(db.get_service_type(None, service_type).await?, Some(expected.clone()));
        assert_eq!(db.get_service_types(None).await?, vec![expected]);

        Ok(())
    }

    #[tokio::test]
    async fn test_service_type_change_on_unknown_type_fails() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;

        let result = db
            .set_service_type_owner(None, ServiceType::GVPN_EXIT, Some(random_address()), 100, 0, 0)
            .await;
        assert!(matches!(result, Err(DbSqlError::EntityNotFound(_))));

        Ok(())
    }

    #[tokio::test]
    async fn test_service_type_state_replay_is_idempotent() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;

        let service_type = ServiceType::GVPN_EXIT;
        let owner = random_address();

        db.register_service_type(None, service_type, owner, 100, 0, 0).await?;
        db.set_service_type_registration_burn(None, service_type, HoprBalance::from(5), 200, 0, 0)
            .await?;

        db.set_service_type_registration_burn(None, service_type, HoprBalance::from(5), 200, 0, 0)
            .await?;
        db.register_service_type(None, service_type, owner, 100, 0, 0).await?;

        let state_rows = ServiceTypeState::find().count(db.conn(crate::TargetDb::Index)).await?;
        assert_eq!(state_rows, 2);
        assert_eq!(
            ServiceTypeEntity::find().count(db.conn(crate::TargetDb::Index)).await?,
            1
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_registry_config_tracks_latest_log_only() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;

        assert_eq!(
            db.get_service_registry_config(None).await?,
            ServiceRegistryConfigInfo {
                type_registration_fee: HoprBalance::from(0),
                node_safe_registry: None,
            }
        );

        let registry = random_address();
        db.set_type_registration_fee(None, HoprBalance::from(10), 100, 0, 0)
            .await?;
        db.set_node_safe_registry(None, registry, 200, 0, 0).await?;

        assert_eq!(
            db.get_service_registry_config(None).await?,
            ServiceRegistryConfigInfo {
                type_registration_fee: HoprBalance::from(10),
                node_safe_registry: Some(registry),
            }
        );

        // Replaying the older fee log must not clobber the newer pointer, and a stale fee value
        // must not win over the applied one.
        db.set_type_registration_fee(None, HoprBalance::from(10), 100, 0, 0)
            .await?;
        db.set_type_registration_fee(None, HoprBalance::from(20), 100, 0, 1)
            .await?;

        assert_eq!(
            db.get_service_registry_config(None).await?,
            ServiceRegistryConfigInfo {
                type_registration_fee: HoprBalance::from(10),
                node_safe_registry: Some(registry),
            }
        );

        Ok(())
    }
    /// The aggregation resolves the node accounts with one batched query and a `HashMap` join, and
    /// yields `None` for a node that never announced itself.
    #[tokio::test]
    async fn test_service_aggregation_joins_accounts_opportunistically() -> anyhow::Result<()> {
        let db = BlokliDb::new_in_memory().await?;
        let conn = db.conn(crate::TargetDb::Index);

        let announced_node = random_address();
        let silent_node = random_address();
        let gone_node = random_address();
        let owner = random_address();
        let service_type = ServiceType::GVPN_EXIT;

        let account_id = account::ActiveModel {
            chain_key: Set(announced_node.as_ref().to_vec()),
            packet_key: Set("announced-peer".to_string()),
            ..Default::default()
        }
        .insert(conn)
        .await?
        .id;

        db.register_service_type(None, service_type, owner, 50, 0, 0).await?;
        db.set_service_type_registration_burn(None, service_type, HoprBalance::from(9), 60, 0, 0)
            .await?;

        for (index, node) in [announced_node, silent_node, gone_node].into_iter().enumerate() {
            let published = entry(
                service_type,
                node,
                random_address(),
                b"metadata",
                timestamp(0),
                timestamp(0),
            )?;
            db.upsert_service_entry(None, &published, 100, 0, index as u64).await?;
        }
        db.deregister_service_entry(None, service_type, gone_node, 200, 0, 0)
            .await?;

        let aggregated = fetch_service_entries(conn).await?;
        let keyid_by_node: HashMap<String, Option<i64>> = aggregated
            .iter()
            .map(|e| (e.node_address.clone(), e.node_keyid))
            .collect();

        // The deregistered entry is gone, and only the announced node resolves to an account.
        assert_eq!(keyid_by_node.len(), 2);
        assert_eq!(keyid_by_node.get(&announced_node.to_hex()), Some(&Some(account_id)));
        assert_eq!(keyid_by_node.get(&silent_node.to_hex()), Some(&None));
        assert_eq!(keyid_by_node.get(&gone_node.to_hex()), None);

        let by_node = fetch_service_entries_for_nodes(conn, &[silent_node.as_ref().to_vec()]).await?;
        assert_eq!(by_node.len(), 1);
        assert_eq!(by_node[silent_node.as_ref()].len(), 1);

        let for_type = fetch_service_entries_for_type(conn, service_type.as_ref()).await?;
        assert_eq!(for_type.len(), 2);

        let types = fetch_service_types(conn).await?;
        assert_eq!(types.len(), 1);
        assert_eq!(types[0].owner_address, Some(owner.to_hex()));
        assert_eq!(types[0].requirement_address, None);
        assert_eq!(
            HoprBalance::from_be_bytes(types[0].registration_burn.as_slice()),
            HoprBalance::from(9)
        );

        Ok(())
    }
}
