//! Database model to GraphQL type conversions
//!
//! This module contains conversion functions that convert database entity
//! models into GraphQL types. These conversions are kept separate from
//! the type definitions to avoid requiring API clients to depend on database entities.

use blokli_api_types::{Announcement, HoprBalance, NativeBalance, TokenValueString, TransactionStatus};
use blokli_chain_api::transaction_store::TransactionStatus as StoreStatus;
use hopr_primitive_types::prelude::{Address, HoprBalance as PrimitiveHoprBalance, IntoEndian, ToHex};

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

/// Convert database HOPR balance model to GraphQL type
pub fn hopr_balance_from_model(model: blokli_db_entity::hopr_balance::Model) -> HoprBalance {
    HoprBalance {
        address: Address::new(&model.address).to_hex(),
        balance: TokenValueString(PrimitiveHoprBalance::from_be_bytes(&model.balance).amount().to_string()),
    }
}

/// Convert database native balance model to GraphQL type
pub fn native_balance_from_model(model: blokli_db_entity::native_balance::Model) -> NativeBalance {
    NativeBalance {
        address: Address::new(&model.address).to_hex(),
        balance: TokenValueString(PrimitiveHoprBalance::from_be_bytes(&model.balance).amount().to_string()),
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
