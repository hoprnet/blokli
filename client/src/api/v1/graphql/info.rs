use super::TokenValueString;
use super::schema;

#[derive(cynic::QueryFragment, Debug)]
#[cynic(graphql_type = "QueryRoot")]
pub struct QueryChainInfo {
    pub chain_info: ChainInfo,
}

#[derive(cynic::QueryFragment, Debug)]
pub struct ChainInfo {
    pub block_number: i32,
    pub chain_id: i32,
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
