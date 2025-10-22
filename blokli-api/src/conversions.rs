//! Database model to GraphQL type conversions
//!
//! This module contains conversion functions that convert database entity
//! models into GraphQL types. These conversions are kept separate from
//! the type definitions to avoid requiring API clients to depend on database entities.

use blokli_api_types::{Announcement, Channel, ChannelStatus, HoprBalance, NativeBalance, TokenValueString, UInt64};

/// Convert ChannelStatus enum to database integer representation
///
/// This conversion is used when filtering channels by status in database queries.
/// The mapping is:
/// - Closed -> 0
/// - Open -> 1
/// - PendingToClose -> 2
pub fn channel_status_to_i8(status: ChannelStatus) -> i8 {
    match status {
        ChannelStatus::Closed => 0,
        ChannelStatus::Open => 1,
        ChannelStatus::PendingToClose => 2,
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
pub fn channel_from_model(model: blokli_db_entity::channel::Model) -> Channel {
    use blokli_db_entity::conversions::balances::hopr_balance_to_string;

    let balance = TokenValueString(hopr_balance_to_string(&model.balance));

    // epoch is uint24 in Solidity (max 16,777,215), safe cast to i32
    // In practice, epoch should never exceed i32::MAX, but clamp for safety
    #[allow(clippy::cast_possible_truncation)]
    let epoch = model.epoch.clamp(0, i32::MAX as i64) as i32;

    // ticket_index is uint48 in Solidity (max 281,474,976,710,655), fits in u64
    // Cast i64 to u64 (safe: stored values are always non-negative blockchain indices)
    #[allow(clippy::cast_sign_loss)]
    let ticket_index = UInt64(model.ticket_index as u64);

    Channel {
        concrete_channel_id: model.concrete_channel_id,
        source: model.source,
        destination: model.destination,
        balance,
        status: ChannelStatus::from(model.status),
        epoch,
        ticket_index,
        closure_time: model.closure_time,
    }
}

/// Convert database HOPR balance model to GraphQL type
pub fn hopr_balance_from_model(model: blokli_db_entity::hopr_balance::Model) -> HoprBalance {
    use blokli_db_entity::conversions::balances::{address_to_string, balance_to_string};

    HoprBalance {
        address: address_to_string(&model.address),
        balance: TokenValueString(balance_to_string(&model.balance)),
    }
}

/// Convert database native balance model to GraphQL type
pub fn native_balance_from_model(model: blokli_db_entity::native_balance::Model) -> NativeBalance {
    use blokli_db_entity::conversions::balances::{address_to_string, balance_to_string};

    NativeBalance {
        address: address_to_string(&model.address),
        balance: TokenValueString(balance_to_string(&model.balance)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_channel_status_to_i8_mapping() {
        // Verify database encoding matches: 0=Closed, 1=Open, 2=PendingToClose
        assert_eq!(channel_status_to_i8(ChannelStatus::Closed), 0);
        assert_eq!(channel_status_to_i8(ChannelStatus::Open), 1);
        assert_eq!(channel_status_to_i8(ChannelStatus::PendingToClose), 2);
    }

    #[test]
    fn test_channel_status_round_trip() {
        // Verify bidirectional conversion consistency
        assert_eq!(
            ChannelStatus::from(channel_status_to_i8(ChannelStatus::Closed)),
            ChannelStatus::Closed
        );
        assert_eq!(
            ChannelStatus::from(channel_status_to_i8(ChannelStatus::Open)),
            ChannelStatus::Open
        );
        assert_eq!(
            ChannelStatus::from(channel_status_to_i8(ChannelStatus::PendingToClose)),
            ChannelStatus::PendingToClose
        );
    }
}
