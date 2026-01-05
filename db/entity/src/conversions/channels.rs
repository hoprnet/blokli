// Allow casts for Solidity types that fit safely in i64:
// - epoch is uint24 (max 16,777,215)
// - ticket_index is uint48 (max 281,474,976,710,655)
#![allow(clippy::cast_possible_wrap)]

use hopr_internal_types::{channels::ChannelStatus, prelude::ChannelEntry};
use sea_orm::Set;

use crate::{channel, errors::DbEntityError};

/// Extension trait for updating [ChannelStatus] inside [channel::ActiveModel].
/// This is needed as `status` maps to two model members.
pub trait ChannelStatusUpdate {
    /// Update [ChannelStatus] of this active model.
    fn set_status(&mut self, new_status: ChannelStatus);
}

impl ChannelStatusUpdate for channel::ActiveModel {
    fn set_status(&mut self, _new_status: ChannelStatus) {
        // TODO(Phase 2-3): Update to work with channel_state table
        // Status is now stored in channel_state, not channel
        panic!("Channel status updates must now go through channel_state table - not yet implemented");
    }
}

impl TryFrom<&channel::Model> for ChannelStatus {
    type Error = DbEntityError;

    fn try_from(_value: &channel::Model) -> Result<Self, Self::Error> {
        // TODO(Phase 2-3): Update to query channel_state table for current status
        // Channel status is now stored in channel_state, not channel
        Err(DbEntityError::Conversion(
            "Channel status conversion requires querying channel_state table - not yet implemented".into(),
        ))
    }
}

impl TryFrom<&channel::Model> for ChannelEntry {
    type Error = DbEntityError;

    fn try_from(_value: &channel::Model) -> Result<Self, Self::Error> {
        // TODO: After schema change to use foreign keys, this conversion requires
        // database access to look up account addresses from keyids.
        // This needs to be refactored to use a separate function that takes a database connection.
        Err(DbEntityError::Conversion(
            "ChannelEntry conversion from database model requires account lookup - use fetch function instead".into(),
        ))
    }
}

impl TryFrom<channel::Model> for ChannelEntry {
    type Error = DbEntityError;

    fn try_from(value: channel::Model) -> Result<Self, Self::Error> {
        (&value).try_into()
    }
}

impl From<ChannelEntry> for channel::ActiveModel {
    fn from(value: ChannelEntry) -> Self {
        // TODO(Phase 2-3): After schema change, channel identity and state are separate
        // - channel table stores only: concrete_channel_id, source, destination (identity)
        // - channel_state table stores: balance, status, epoch, ticket_index, closure_time (mutable state)
        // This conversion needs to be refactored to:
        // 1. Create channel identity record
        // 2. Create initial channel_state record
        // For now, only create the identity part
        channel::ActiveModel {
            concrete_channel_id: Set(hex::encode(value.get_id())),
            source: Set(0),      // TODO: Need to lookup/create account for value.source address
            destination: Set(0), // TODO: Need to lookup/create account for value.destination address
            ..Default::default()
        }
    }
}
