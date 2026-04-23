//! Safe aggregation utilities using current-state database views

use std::collections::HashMap;

use hopr_types::primitive::primitives::Address;
use sea_orm::{ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter, QueryOrder};

use crate::views::{safe_contract_current, safe_owner_current, safe_threshold_current};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CurrentSafe {
    pub address: Vec<u8>,
    pub module_address: Vec<u8>,
    pub chain_key: Vec<u8>,
    pub threshold: Option<String>,
}

fn current_safe_from_model(
    safe: safe_contract_current::Model,
    thresholds_by_address: &HashMap<Vec<u8>, String>,
) -> CurrentSafe {
    CurrentSafe {
        threshold: thresholds_by_address.get(&safe.address).cloned(),
        address: safe.address,
        module_address: safe.module_address,
        chain_key: safe.chain_key,
    }
}

async fn fetch_thresholds_by_safe_address<C>(conn: &C) -> Result<HashMap<Vec<u8>, String>, sea_orm::DbErr>
where
    C: ConnectionTrait,
{
    Ok(safe_threshold_current::Entity::find()
        .all(conn)
        .await?
        .into_iter()
        .map(|row| (row.safe_address, row.threshold))
        .collect())
}

pub async fn fetch_safe_by_address<C>(conn: &C, safe_address: &[u8]) -> Result<Option<CurrentSafe>, sea_orm::DbErr>
where
    C: ConnectionTrait,
{
    let thresholds_by_address = fetch_thresholds_by_safe_address(conn).await?;
    let current_safe = safe_contract_current::Entity::find()
        .filter(safe_contract_current::Column::Address.eq(safe_address.to_vec()))
        .one(conn)
        .await?;

    Ok(current_safe.map(|safe| current_safe_from_model(safe, &thresholds_by_address)))
}

pub async fn fetch_safe_by_owner<C>(conn: &C, owner_address: &[u8]) -> Result<Option<CurrentSafe>, sea_orm::DbErr>
where
    C: ConnectionTrait,
{
    let safe_owner = safe_owner_current::Entity::find()
        .filter(safe_owner_current::Column::OwnerAddress.eq(owner_address.to_vec()))
        .order_by_asc(safe_owner_current::Column::SafeAddress)
        .one(conn)
        .await?;

    match safe_owner {
        Some(safe_owner) => fetch_safe_by_address(conn, &safe_owner.safe_address).await,
        None => Ok(None),
    }
}

pub async fn fetch_safe_addresses<C>(conn: &C, owner_address: Option<&[u8]>) -> Result<Vec<Vec<u8>>, sea_orm::DbErr>
where
    C: ConnectionTrait,
{
    if let Some(owner_address) = owner_address {
        return Ok(safe_owner_current::Entity::find()
            .filter(safe_owner_current::Column::OwnerAddress.eq(owner_address.to_vec()))
            .order_by_asc(safe_owner_current::Column::SafeAddress)
            .all(conn)
            .await?
            .into_iter()
            .map(|row| row.safe_address)
            .collect());
    }

    Ok(safe_contract_current::Entity::find()
        .all(conn)
        .await?
        .into_iter()
        .map(|row| row.address)
        .collect())
}

pub async fn fetch_safe_threshold_by_address<C>(conn: &C, safe_address: &[u8]) -> Result<Option<String>, sea_orm::DbErr>
where
    C: ConnectionTrait,
{
    Ok(safe_threshold_current::Entity::find()
        .filter(safe_threshold_current::Column::SafeAddress.eq(safe_address.to_vec()))
        .one(conn)
        .await?
        .map(|row| row.threshold))
}

pub async fn fetch_safe_owners<C>(conn: &C, safe_address: &[u8]) -> Result<Vec<Address>, sea_orm::DbErr>
where
    C: ConnectionTrait,
{
    Ok(safe_owner_current::Entity::find()
        .filter(safe_owner_current::Column::SafeAddress.eq(safe_address.to_vec()))
        .order_by_asc(safe_owner_current::Column::OwnerAddress)
        .all(conn)
        .await?
        .into_iter()
        .filter_map(|row| Address::try_from(row.owner_address.as_slice()).ok())
        .collect())
}

pub async fn fetch_safe_owners_by_safe<C>(conn: &C) -> Result<HashMap<Vec<u8>, Vec<Address>>, sea_orm::DbErr>
where
    C: ConnectionTrait,
{
    let rows = safe_owner_current::Entity::find()
        .order_by_asc(safe_owner_current::Column::SafeAddress)
        .order_by_asc(safe_owner_current::Column::OwnerAddress)
        .all(conn)
        .await?;

    let mut owners_by_safe: HashMap<Vec<u8>, Vec<Address>> = HashMap::new();
    for row in rows {
        if let Ok(owner_address) = Address::try_from(row.owner_address.as_slice()) {
            owners_by_safe.entry(row.safe_address).or_default().push(owner_address);
        }
    }

    Ok(owners_by_safe)
}

pub async fn fetch_all_current_safes<C>(conn: &C) -> Result<Vec<CurrentSafe>, sea_orm::DbErr>
where
    C: ConnectionTrait,
{
    let thresholds_by_address = fetch_thresholds_by_safe_address(conn).await?;
    Ok(safe_contract_current::Entity::find()
        .all(conn)
        .await?
        .into_iter()
        .map(|safe| current_safe_from_model(safe, &thresholds_by_address))
        .collect())
}
