use hex::ToHex;

use super::{Uint64, schema};
use crate::api::v1::TicketSelector;

/// GraphQL subscription variables for the `ticketRedeemed` subscription.
///
/// At most one filter field should be set; all `None` matches every redemption.
/// Constructed via the [`From<TicketSelector>`] impl.
#[derive(cynic::QueryVariables, Default)]
pub struct TicketRedeemedVariables {
    pub channel_id: Option<cynic::Id>,
    pub issuer_address: Option<cynic::Id>,
    pub recipient_address: Option<cynic::Id>,
}

impl From<TicketSelector> for TicketRedeemedVariables {
    fn from(value: TicketSelector) -> Self {
        match value {
            TicketSelector::ChannelId(channel_id) => TicketRedeemedVariables {
                channel_id: Some(channel_id.encode_hex::<String>().into()),
                ..Default::default()
            },
            TicketSelector::IssuerAddress(address) => TicketRedeemedVariables {
                issuer_address: Some(address.encode_hex::<String>().into()),
                ..Default::default()
            },
            TicketSelector::RecipientAddress(address) => TicketRedeemedVariables {
                recipient_address: Some(address.encode_hex::<String>().into()),
                ..Default::default()
            },
            TicketSelector::Any => TicketRedeemedVariables::default(),
        }
    }
}

/// cynic query fragment for the `ticketRedeemed` GraphQL subscription.
///
/// Use [`TicketRedeemedVariables`] to set optional filters before executing.
#[derive(cynic::QueryFragment, Debug)]
#[cynic(graphql_type = "SubscriptionRoot", variables = "TicketRedeemedVariables")]
pub struct SubscribeTicketRedeemed {
    #[arguments(channelId: $channel_id, issuerAddress: $issuer_address, recipientAddress: $recipient_address)]
    pub ticket_redeemed: RedeemTicketDetails,
}

/// Details of a single on-chain ticket redemption event.
///
/// Returned as stream items by [`BlokliSubscriptionClient::subscribe_ticket_redeemed`].
/// The `epoch` and `index` fields identify which ticket was redeemed; `result` indicates
/// whether the chain accepted or rejected it.
#[derive(cynic::QueryFragment, Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct RedeemTicketDetails {
    pub issuer_address: String,
    pub recipient_address: String,
    pub epoch: Uint64,
    pub index: Uint64,
    pub result: RedemptionResult,
}

/// Outcome of an on-chain ticket redemption attempt.
#[derive(cynic::Enum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum RedemptionResult {
    #[cynic(rename = "REDEEMED")]
    Redeemed,
    #[cynic(rename = "REJECTED")]
    Rejected,
}
