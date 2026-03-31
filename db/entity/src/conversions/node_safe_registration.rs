//! Query helpers for node-safe registration lookups

use hopr_types::primitive::primitives::Address;
use sea_orm::{ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter};

use crate::hopr_node_safe_registration;

/// Fetch all node addresses registered to the given safe.
///
/// Returns the registered node addresses in the order returned by the database.
/// Entries whose stored bytes cannot be parsed as a valid 20-byte address are silently dropped.
pub async fn fetch_registered_nodes_for_safe<C>(conn: &C, safe_address: &[u8]) -> Result<Vec<Address>, sea_orm::DbErr>
where
    C: ConnectionTrait,
{
    let registrations = hopr_node_safe_registration::Entity::find()
        .filter(hopr_node_safe_registration::Column::SafeAddress.eq(safe_address.to_vec()))
        .all(conn)
        .await?;

    Ok(registrations
        .into_iter()
        .filter_map(|reg| Address::try_from(reg.node_address.as_slice()).ok())
        .collect())
}
