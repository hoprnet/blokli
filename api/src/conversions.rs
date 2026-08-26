//! Database model to GraphQL type conversions
//!
//! This module contains conversion functions that convert internal domain types
//! into GraphQL types. These conversions are kept separate from the type
//! definitions to maintain clean module boundaries — the chain layer remains
//! independent of GraphQL presentation types.

use blokli_api_types::{
    Announcement, Hex32, QueryFailedError, SafeExecution, ServiceEntry, ServiceRegistryConfig, ServiceTypeInfo,
    TokenValueString, Transaction, TransactionStatus as GqlTransactionStatus, UInt64,
};
use blokli_chain_api::transaction_store::{
    SafeExecutionResult, TransactionRecord, TransactionStatus as StoreTransactionStatus,
};
use blokli_db_entity::{
    conversions::service_aggregation::{AggregatedServiceEntry, AggregatedServiceType},
    service_registry_config,
};
use hopr_bindings::exports::alloy::hex;
use hopr_types::{
    internal::prelude::ServiceType,
    primitive::{
        prelude::HoprBalance as PrimitiveHoprBalance,
        primitives::Address,
        traits::{IntoEndian, ToHex},
    },
};
use sea_orm::entity::prelude::DateTimeWithTimeZone;

use crate::errors;

/// Convert database announcement model to GraphQL type
pub fn announcement_from_model(model: blokli_db_entity::announcement::Model) -> Announcement {
    Announcement {
        id: model.id,
        account_id: model.account_id,
        multiaddress: model.multiaddress,
        published_block: model.published_block.to_string(),
    }
}

/// Convert a [`TransactionRecord`] to the GraphQL [`Transaction`] type
pub fn transaction_from_record(record: TransactionRecord) -> Transaction {
    Transaction {
        id: async_graphql::ID::from(record.id.to_string()),
        status: convert_transaction_status(record.status),
        submitted_at: record.submitted_at,
        transaction_hash: Hex32(record.transaction_hash.to_hex()),
        safe_execution: convert_safe_execution(record.safe_execution),
    }
}

/// Convert [`StoreTransactionStatus`] to GraphQL [`GqlTransactionStatus`]
pub fn convert_transaction_status(status: StoreTransactionStatus) -> GqlTransactionStatus {
    match status {
        StoreTransactionStatus::Submitted => GqlTransactionStatus::Submitted,
        StoreTransactionStatus::Confirmed => GqlTransactionStatus::Confirmed,
        StoreTransactionStatus::Reverted => GqlTransactionStatus::Reverted,
        StoreTransactionStatus::Timeout => GqlTransactionStatus::Timeout,
        StoreTransactionStatus::ValidationFailed => GqlTransactionStatus::ValidationFailed,
        StoreTransactionStatus::SubmissionFailed => GqlTransactionStatus::SubmissionFailed,
    }
}

/// Convert an optional [`SafeExecutionResult`] to the GraphQL [`SafeExecution`] type
pub fn convert_safe_execution(result: Option<SafeExecutionResult>) -> Option<SafeExecution> {
    result.map(|r| SafeExecution {
        success: r.success,
        safe_tx_hash: r.safe_tx_hash.map(|h| Hex32(h.to_hex())),
        revert_reason: r.revert_reason,
    })
}

/// Convert an aggregated service registry entry into the GraphQL type
///
/// The service type renders as its ASCII name when it follows the right-padded ASCII convention
/// and as `0x`-prefixed hex otherwise, which is what [`ServiceType`]'s own `Display` does. The
/// metadata stays opaque: its schema belongs to the service type, not to the registry, so it is
/// exposed as `0x`-prefixed hex.
pub fn service_entry_from_aggregate(entry: AggregatedServiceEntry) -> Result<ServiceEntry, QueryFailedError> {
    let service_type = ServiceType::try_from(entry.service_type.as_slice())
        .map_err(|e| errors::invalid_db_data("service_entry.service_type", &e.to_string()))?;

    Ok(ServiceEntry {
        service_type: service_type.to_string(),
        node: entry.node_address,
        safe: entry.safe_address,
        metadata: hex::encode_prefixed(&entry.metadata),
        registered_at: unix_seconds(entry.registered_at)?,
        updated_at: unix_seconds(entry.updated_at)?,
    })
}

/// Convert a stored timestamp into the Unix seconds the GraphQL schema exposes
///
/// A registry timestamp is a `uint48` on-chain and can never be negative; a negative value here
/// means the row was not written by the indexer.
fn unix_seconds(time: DateTimeWithTimeZone) -> Result<UInt64, QueryFailedError> {
    u64::try_from(time.timestamp())
        .map(UInt64)
        .map_err(|_| errors::invalid_db_data("service_entry_state timestamp", "precedes the Unix epoch"))
}

/// Convert the registry-wide configuration row into the GraphQL type
///
/// The node-safe registry pointer reads as the zero address until the registry has emitted it,
/// which mirrors how the contract itself reads before initialization.
pub fn service_registry_config_from_model(
    model: service_registry_config::Model,
) -> Result<ServiceRegistryConfig, QueryFailedError> {
    let node_safe_registry = match model.node_safe_registry.as_deref() {
        Some(bytes) => Address::try_from(bytes)
            .map_err(|e| errors::invalid_db_data("service_registry_config.node_safe_registry", &e.to_string()))?,
        None => Address::default(),
    };

    Ok(ServiceRegistryConfig {
        type_registration_fee: TokenValueString(
            PrimitiveHoprBalance::from_be_bytes(model.type_registration_fee.as_slice()).to_string(),
        ),
        node_safe_registry: node_safe_registry.to_hex(),
    })
}

/// Convert an aggregated service type into the GraphQL type
///
/// A `None` owner means the type was abandoned and a `None` requirement means the type is open to
/// any node; both encode the zero-address sentinels the registry uses on-chain.
pub fn service_type_from_aggregate(info: AggregatedServiceType) -> Result<ServiceTypeInfo, QueryFailedError> {
    let service_type = ServiceType::try_from(info.service_type.as_slice())
        .map_err(|e| errors::invalid_db_data("service_type.service_type", &e.to_string()))?;

    Ok(ServiceTypeInfo {
        service_type: service_type.to_string(),
        owner: info.owner_address,
        requirement: info.requirement_address,
        registration_burn: TokenValueString(
            PrimitiveHoprBalance::from_be_bytes(info.registration_burn.as_slice()).to_string(),
        ),
        update_burn: TokenValueString(PrimitiveHoprBalance::from_be_bytes(info.update_burn.as_slice()).to_string()),
    })
}

#[cfg(test)]
mod tests {
    use blokli_api_types::ChannelStatus;

    #[test]
    fn test_channel_status_to_i16_mapping() {
        // Verify database encoding matches: 0=Closed, 1=Open, 2=PendingToClose
        assert_eq!(i16::from(ChannelStatus::Closed), 0);
        assert_eq!(i16::from(ChannelStatus::Open), 1);
        assert_eq!(i16::from(ChannelStatus::PendingToClose), 2);
    }

    #[test]
    fn test_channel_status_round_trip() {
        // Verify bidirectional conversion consistency
        assert_eq!(
            ChannelStatus::from(i16::from(ChannelStatus::Closed)),
            ChannelStatus::Closed
        );
        assert_eq!(ChannelStatus::from(i16::from(ChannelStatus::Open)), ChannelStatus::Open);
        assert_eq!(
            ChannelStatus::from(i16::from(ChannelStatus::PendingToClose)),
            ChannelStatus::PendingToClose
        );
    }
}
