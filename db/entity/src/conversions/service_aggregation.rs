//! Service registry aggregation utilities using the `service_entry_current` and
//! `service_type_current` database views.
//!
//! Every fetch here resolves its related rows with one extra query and a `HashMap` join. The node
//! of an entry is a raw chain address rather than a foreign key — the registry does not require a
//! node to have announced itself — so the `account` lookup is opportunistic and yields `None` for
//! a node with no `account` row.

use std::collections::HashMap;

use hopr_types::primitive::{primitives::Address, traits::ToHex};
use sea_orm::{ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter, entity::prelude::DateTimeWithTimeZone};

use crate::{
    codegen::account,
    views::{service_entry_current, service_type_current},
};

fn bytes_to_address_hex(bytes: &[u8]) -> Result<String, sea_orm::DbErr> {
    let addr_bytes: [u8; 20] = bytes.try_into().map_err(|_| {
        sea_orm::DbErr::Custom(format!(
            "Invalid address length: expected 20 bytes, got {}",
            bytes.len()
        ))
    })?;
    Ok(Address::new(&addr_bytes).to_hex())
}

/// Reports a live entry row that is missing a column only a deregistration tombstone may omit.
fn missing_column(service_entry_id: i64, column: &str) -> sea_orm::DbErr {
    sea_orm::DbErr::Custom(format!("service entry {service_entry_id} is live but has no {column}"))
}

/// A live service registry entry together with the node's account key id, when the node has one.
#[derive(Debug, Clone)]
pub struct AggregatedServiceEntry {
    pub service_entry_id: i64,
    pub service_type_id: i64,
    /// The raw 32-byte service type id.
    pub service_type: Vec<u8>,
    pub node_address: String,
    /// Key id of the node's `account` row, or `None` when the node never announced itself.
    pub node_keyid: Option<i64>,
    pub safe_address: String,
    pub metadata: Vec<u8>,
    pub registered_at: DateTimeWithTimeZone,
    pub updated_at: DateTimeWithTimeZone,
    pub published_block: i64,
    pub published_tx_index: i64,
    pub published_log_index: i64,
}

/// The latest state of a service type.
#[derive(Debug, Clone)]
pub struct AggregatedServiceType {
    pub service_type_id: i64,
    /// The raw 32-byte service type id.
    pub service_type: Vec<u8>,
    /// `None` when the type was abandoned.
    pub owner_address: Option<String>,
    /// `None` when the type is open to any node.
    pub requirement_address: Option<String>,
    /// Big-endian 32-byte burn amounts.
    pub registration_burn: Vec<u8>,
    pub update_burn: Vec<u8>,
    pub published_block: i64,
    pub published_tx_index: i64,
    pub published_log_index: i64,
}

/// Given a list of `service_entry_current` view rows, batch-resolve the node accounts and build
/// the aggregated entries.
async fn aggregate_service_entries<C>(
    conn: &C,
    current_entries: Vec<service_entry_current::Model>,
) -> Result<Vec<AggregatedServiceEntry>, sea_orm::DbErr>
where
    C: ConnectionTrait,
{
    if current_entries.is_empty() {
        return Ok(Vec::new());
    }

    let node_addresses: Vec<Vec<u8>> = current_entries.iter().map(|e| e.node_address.clone()).collect();

    // One query for every node, then an in-memory join: per-row lookups in the loop below are
    // forbidden.
    let accounts = account::Entity::find()
        .filter(account::Column::ChainKey.is_in(node_addresses))
        .all(conn)
        .await?;

    let keyid_by_node: HashMap<Vec<u8>, i64> = accounts.into_iter().map(|a| (a.chain_key, a.id)).collect();

    current_entries
        .into_iter()
        .map(|row| {
            let safe_address = row
                .safe_address
                .as_ref()
                .ok_or_else(|| missing_column(row.service_entry_id, "safe address"))
                .and_then(|addr| bytes_to_address_hex(addr))?;

            Ok(AggregatedServiceEntry {
                service_entry_id: row.service_entry_id,
                service_type_id: row.service_type_id,
                service_type: row.service_type,
                node_address: bytes_to_address_hex(&row.node_address)?,
                node_keyid: keyid_by_node.get(&row.node_address).copied(),
                safe_address,
                metadata: row
                    .metadata
                    .ok_or_else(|| missing_column(row.service_entry_id, "metadata"))?,
                registered_at: row
                    .registered_at
                    .ok_or_else(|| missing_column(row.service_entry_id, "registration timestamp"))?,
                updated_at: row
                    .updated_at
                    .ok_or_else(|| missing_column(row.service_entry_id, "update timestamp"))?,
                published_block: row.published_block,
                published_tx_index: row.published_tx_index,
                published_log_index: row.published_log_index,
            })
        })
        .collect()
}

/// Fetch every live service entry.
///
/// Deregistration tombstones are excluded, so the result holds only entries that currently exist
/// on-chain.
pub async fn fetch_service_entries<C>(conn: &C) -> Result<Vec<AggregatedServiceEntry>, sea_orm::DbErr>
where
    C: ConnectionTrait,
{
    let current_entries = service_entry_current::Entity::find()
        .filter(service_entry_current::Column::Deregistered.eq(false))
        .all(conn)
        .await?;

    aggregate_service_entries(conn, current_entries).await
}

/// Fetch the live service entries of a single service type.
///
/// # Arguments
/// * `conn` - Database connection
/// * `service_type` - The raw 32-byte service type id
pub async fn fetch_service_entries_for_type<C>(
    conn: &C,
    service_type: &[u8],
) -> Result<Vec<AggregatedServiceEntry>, sea_orm::DbErr>
where
    C: ConnectionTrait,
{
    let current_entries = service_entry_current::Entity::find()
        .filter(service_entry_current::Column::ServiceType.eq(service_type.to_vec()))
        .filter(service_entry_current::Column::Deregistered.eq(false))
        .all(conn)
        .await?;

    aggregate_service_entries(conn, current_entries).await
}

/// Fetch the live service entries of the given nodes, keyed by node chain address.
///
/// Nodes without a live entry are absent from the map rather than mapped to an empty vector.
///
/// # Arguments
/// * `conn` - Database connection
/// * `node_addresses` - Raw 20-byte node chain addresses
pub async fn fetch_service_entries_for_nodes<C>(
    conn: &C,
    node_addresses: &[Vec<u8>],
) -> Result<HashMap<Vec<u8>, Vec<AggregatedServiceEntry>>, sea_orm::DbErr>
where
    C: ConnectionTrait,
{
    if node_addresses.is_empty() {
        return Ok(HashMap::new());
    }

    let current_entries = service_entry_current::Entity::find()
        .filter(service_entry_current::Column::NodeAddress.is_in(node_addresses.to_vec()))
        .filter(service_entry_current::Column::Deregistered.eq(false))
        .all(conn)
        .await?;

    // Remember the raw addresses before aggregation turns them into hex strings.
    let raw_node_by_entry_id: HashMap<i64, Vec<u8>> = current_entries
        .iter()
        .map(|e| (e.service_entry_id, e.node_address.clone()))
        .collect();

    let aggregated = aggregate_service_entries(conn, current_entries).await?;

    let mut entries_by_node: HashMap<Vec<u8>, Vec<AggregatedServiceEntry>> = HashMap::new();
    for entry in aggregated {
        let Some(node) = raw_node_by_entry_id.get(&entry.service_entry_id).cloned() else {
            continue;
        };
        entries_by_node.entry(node).or_default().push(entry);
    }

    Ok(entries_by_node)
}

/// Fetch the latest state of every service type.
pub async fn fetch_service_types<C>(conn: &C) -> Result<Vec<AggregatedServiceType>, sea_orm::DbErr>
where
    C: ConnectionTrait,
{
    let current_types = service_type_current::Entity::find().all(conn).await?;
    aggregate_service_types(current_types)
}

/// Fetch the latest state of the given service types.
///
/// # Arguments
/// * `conn` - Database connection
/// * `service_types` - Raw 32-byte service type ids
pub async fn fetch_service_types_by_id<C>(
    conn: &C,
    service_types: &[Vec<u8>],
) -> Result<Vec<AggregatedServiceType>, sea_orm::DbErr>
where
    C: ConnectionTrait,
{
    if service_types.is_empty() {
        return Ok(Vec::new());
    }

    let current_types = service_type_current::Entity::find()
        .filter(service_type_current::Column::ServiceType.is_in(service_types.to_vec()))
        .all(conn)
        .await?;

    aggregate_service_types(current_types)
}

fn aggregate_service_types(
    current_types: Vec<service_type_current::Model>,
) -> Result<Vec<AggregatedServiceType>, sea_orm::DbErr> {
    current_types
        .into_iter()
        .map(|row| {
            Ok(AggregatedServiceType {
                service_type_id: row.service_type_id,
                service_type: row.service_type,
                owner_address: row
                    .owner_address
                    .as_ref()
                    .map(|addr| bytes_to_address_hex(addr))
                    .transpose()?,
                requirement_address: row
                    .requirement_address
                    .as_ref()
                    .map(|addr| bytes_to_address_hex(addr))
                    .transpose()?,
                registration_burn: row.registration_burn,
                update_burn: row.update_burn,
                published_block: row.published_block,
                published_tx_index: row.published_tx_index,
                published_log_index: row.published_log_index,
            })
        })
        .collect()
}
