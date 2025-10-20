use hopr_internal_types::{channels::ChannelStatus, prelude::ChannelEntry};
use hopr_primitive_types::prelude::{IntoEndian, ToHex};
use sea_orm::Set;

use crate::{channel, errors::DbEntityError};

/// Extension trait for updating [ChannelStatus] inside [channel::ActiveModel].
/// This is needed as `status` maps to two model members.
pub trait ChannelStatusUpdate {
    /// Update [ChannelStatus] of this active model.
    fn set_status(&mut self, new_status: ChannelStatus);
}

impl ChannelStatusUpdate for channel::ActiveModel {
    fn set_status(&mut self, new_status: ChannelStatus) {
        self.status = Set(i8::from(new_status));
        if let ChannelStatus::PendingToClose(t) = new_status {
            self.closure_time = Set(Some(chrono::DateTime::<chrono::Utc>::from(t)))
        }
    }
}

impl TryFrom<&channel::Model> for ChannelStatus {
    type Error = DbEntityError;

    fn try_from(value: &channel::Model) -> Result<Self, Self::Error> {
        match value.status {
            0 => Ok(ChannelStatus::Closed),
            1 => Ok(ChannelStatus::Open),
            2 => value
                .closure_time
                .ok_or(DbEntityError::Conversion(
                    "channel is pending to close but without closure time".into(),
                ))
                .map(|time| ChannelStatus::PendingToClose(time.into())),
            _ => Err(DbEntityError::Conversion("invalid channel status value".into())),
        }
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
        // TODO: After schema change to use foreign keys, this conversion requires
        // database access to look up or create accounts and get their keyids.
        // This needs to be refactored to use a separate function that takes a database connection.
        // For now, using placeholder values that will cause database constraint violations.
        let mut ret = channel::ActiveModel {
            concrete_channel_id: Set(value.get_id().to_hex()),
            source: Set(0),      // TODO: Need to lookup/create account for value.source address
            destination: Set(0), // TODO: Need to lookup/create account for value.destination address
            balance: Set(value.balance.amount().to_be_bytes().into()),
            epoch: Set(value.channel_epoch.as_u64() as i64),
            ticket_index: Set(value.ticket_index.as_u64() as i64),
            ..Default::default()
        };
        ret.set_status(value.status);
        ret
    }
}
