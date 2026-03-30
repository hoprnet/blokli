//! Database model to GraphQL type conversions
//!
//! This module contains conversion functions that convert internal domain types
//! into GraphQL types. These conversions are kept separate from the type
//! definitions to maintain clean module boundaries — the chain layer remains
//! independent of GraphQL presentation types.

use blokli_api_types::{Announcement, Hex32, SafeExecution, Transaction, TransactionStatus as GqlTransactionStatus};
use blokli_chain_api::transaction_store::{
    SafeExecutionResult, TransactionRecord, TransactionStatus as StoreTransactionStatus,
};
use hopr_types::primitive::traits::ToHex;

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
