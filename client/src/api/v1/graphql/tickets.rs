use super::{Uint64, schema};

#[derive(cynic::QueryVariables, Default)]
pub struct TicketRedeemedVariables {
    pub channel_id: Option<cynic::Id>,
    pub issuer_address: Option<cynic::Id>,
    pub recepient_address: Option<cynic::Id>,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(graphql_type = "SubscriptionRoot", variables = "TicketRedeemedVariables")]
pub struct SubscribeTicketRedeemed {
    #[arguments(channelId: $channel_id, issuerAddress: $issuer_address, recepientAddress: $recepient_address)]
    pub ticket_redeemed: RedeemTicketDetails,
}

#[derive(cynic::QueryFragment, Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct RedeemTicketDetails {
    pub issuer_address: String,
    pub recepient_address: String,
    pub epoch: Uint64,
    pub index: Uint64,
    pub result: RedemptionResult,
}

#[derive(cynic::Enum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum RedemptionResult {
    #[cynic(rename = "REDEEMED")]
    Redeemed,
    #[cynic(rename = "REJECTED")]
    Rejected,
}
