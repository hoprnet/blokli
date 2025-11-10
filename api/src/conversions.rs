//! Database model to GraphQL type conversions
//!
//! This module contains conversion functions that convert database entity
//! models into GraphQL types. These conversions are kept separate from
//! the type definitions to avoid requiring API clients to depend on database entities.

use blokli_api_types::{Announcement, Channel, HoprBalance, NativeBalance, TokenValueString, TransactionStatus};
use blokli_chain_api::transaction_store::TransactionStatus as StoreStatus;
use blokli_db_entity::conversions::balances::{address_to_string, balance_to_string};

/// Convert store TransactionStatus to GraphQL TransactionStatus
pub fn store_status_to_graphql(status: StoreStatus) -> TransactionStatus {
    match status {
        StoreStatus::Pending => TransactionStatus::Pending,
        StoreStatus::Submitted => TransactionStatus::Submitted,
        StoreStatus::Confirmed => TransactionStatus::Confirmed,
        StoreStatus::Reverted => TransactionStatus::Reverted,
        StoreStatus::Timeout => TransactionStatus::Timeout,
        StoreStatus::ValidationFailed => TransactionStatus::ValidationFailed,
        StoreStatus::SubmissionFailed => TransactionStatus::SubmissionFailed,
    }
}

/// Convert database announcement model to GraphQL type
pub fn announcement_from_model(model: blokli_db_entity::announcement::Model) -> Announcement {
    Announcement {
        id: model.id,
        account_id: model.account_id,
        multiaddress: model.multiaddress,
        published_block: model.published_block.to_string(),
    }
}

/// Convert database channel model to GraphQL type
///
/// TODO(Phase 2-3): This function needs to be refactored to query channel_state table
/// Channel state fields (balance, status, epoch, ticket_index, closure_time) are now stored
/// in channel_state table, not channel table. This requires database access to join with
/// channel_state or use the channel_current view.
pub fn channel_from_model(_model: blokli_db_entity::channel::Model) -> Channel {
    // TODO(Phase 2-3): Implement proper channel state lookup from channel_state table
    // For now, this function cannot be used until we implement the state lookup
    panic!(
        "channel_from_model requires refactoring to query channel_state table - use channel queries that join with \
         channel_state instead"
    )
}

/// Convert database HOPR balance model to GraphQL type
pub fn hopr_balance_from_model(model: blokli_db_entity::hopr_balance::Model) -> HoprBalance {
    HoprBalance {
        address: address_to_string(&model.address),
        balance: TokenValueString(balance_to_string(&model.balance)),
    }
}

/// Convert database native balance model to GraphQL type
pub fn native_balance_from_model(model: blokli_db_entity::native_balance::Model) -> NativeBalance {
    NativeBalance {
        address: address_to_string(&model.address),
        balance: TokenValueString(balance_to_string(&model.balance)),
    }
}

#[cfg(test)]
mod tests {
    use blokli_api_types::ChannelStatus;

    #[test]
    fn test_channel_status_to_i8_mapping() {
        // Verify database encoding matches: 0=Closed, 1=Open, 2=PendingToClose
        assert_eq!(i8::from(ChannelStatus::Closed), 0);
        assert_eq!(i8::from(ChannelStatus::Open), 1);
        assert_eq!(i8::from(ChannelStatus::PendingToClose), 2);
    }

    #[test]
    fn test_channel_status_round_trip() {
        // Verify bidirectional conversion consistency
        assert_eq!(
            ChannelStatus::from(i8::from(ChannelStatus::Closed)),
            ChannelStatus::Closed
        );
        assert_eq!(ChannelStatus::from(i8::from(ChannelStatus::Open)), ChannelStatus::Open);
        assert_eq!(
            ChannelStatus::from(i8::from(ChannelStatus::PendingToClose)),
            ChannelStatus::PendingToClose
        );
    }
}
