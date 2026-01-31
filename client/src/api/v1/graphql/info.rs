use super::{QueryFailedError, TokenValueString, Uint64, schema};

#[derive(cynic::QueryFragment, Debug)]
#[cynic(graphql_type = "QueryRoot")]
pub struct QueryChainInfo {
    pub chain_info: ChainInfoResult,
}

#[derive(cynic::QueryFragment, Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct ChainInfo {
    pub channel_closure_grace_period: Uint64,
    pub channel_dst: Option<String>,
    pub block_number: i32,
    pub chain_id: i32,
    pub ledger_dst: Option<String>,
    pub min_ticket_winning_probability: f64,
    pub key_binding_fee: TokenValueString,
    pub safe_registry_dst: Option<String>,
    pub ticket_price: TokenValueString,
    pub network: String,
    pub contract_addresses: ContractAddressMap,
    pub expected_block_time: Uint64,
    pub finality: Uint64,
}

#[derive(cynic::Scalar, Debug, Clone, PartialEq, Eq)]
pub struct ContractAddressMap(pub String);

#[derive(cynic::InlineFragments, Debug)]
pub enum ChainInfoResult {
    ChainInfo(ChainInfo),
    QueryFailedError(QueryFailedError),
    #[cynic(fallback)]
    Unknown,
}

impl From<ChainInfoResult> for Result<ChainInfo, crate::errors::BlokliClientError> {
    fn from(value: ChainInfoResult) -> Self {
        match value {
            ChainInfoResult::ChainInfo(info) => Ok(info),
            ChainInfoResult::QueryFailedError(e) => Err(e.into()),
            ChainInfoResult::Unknown => Err(crate::errors::ErrorKind::NoData.into()),
        }
    }
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(graphql_type = "SubscriptionRoot")]
pub struct SubscribeTicketParams {
    pub ticket_parameters_updated: TicketParameters,
}

#[derive(cynic::QueryFragment, Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct TicketParameters {
    pub min_ticket_winning_probability: f64,
    pub ticket_price: TokenValueString,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(graphql_type = "QueryRoot")]
pub struct QueryVersion {
    pub version: String,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(graphql_type = "QueryRoot")]
pub struct QueryHealth {
    pub health: String,
}
