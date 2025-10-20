//! Database model to GraphQL type conversions
//!
//! This module contains conversion functions that convert database entity
//! models into GraphQL types. These conversions are kept separate from
//! the type definitions to avoid requiring API clients to depend on database entities.

use blokli_api_types::{Announcement, Channel, ChannelStatus, HoprBalance, NativeBalance, TokenValueString};

/// Convert database announcement model to GraphQL type
pub fn announcement_from_model(model: blokli_db_entity::announcement::Model) -> Announcement {
    Announcement {
        id: model.id,
        account_id: model.account_id,
        multiaddress: model.multiaddress,
        published_block: hex::encode(&model.published_block),
    }
}

/// Convert database channel model to GraphQL type
pub fn channel_from_model(model: blokli_db_entity::channel::Model) -> Channel {
    use blokli_db_entity::conversions::balances::{bytes_to_u64, hopr_balance_to_string};

    let balance = TokenValueString(hopr_balance_to_string(&model.balance));
    let epoch = bytes_to_u64(&model.epoch) as i32;
    let ticket_index = bytes_to_u64(&model.ticket_index) as i32;

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
    use blokli_db_entity::conversions::balances::balance_to_string;

    HoprBalance {
        address: model.address,
        balance: TokenValueString(balance_to_string(&model.balance)),
    }
}

/// Convert database native balance model to GraphQL type
pub fn native_balance_from_model(model: blokli_db_entity::native_balance::Model) -> NativeBalance {
    use blokli_db_entity::conversions::balances::balance_to_string;

    NativeBalance {
        address: model.address,
        balance: TokenValueString(balance_to_string(&model.balance)),
    }
}
