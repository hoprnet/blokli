//! SeaORM entity for the `account_current` database view
//!
//! This view returns one row per account with the latest state, using window functions
//! to select the most recent `account_state` entry by `(published_block, published_tx_index, published_log_index)`.

use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "account_current")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub account_id: i64,
    #[sea_orm(column_type = "Binary(20)")]
    pub chain_key: Vec<u8>,
    pub packet_key: String,
    #[sea_orm(column_type = "Binary(20)", nullable)]
    pub safe_address: Option<Vec<u8>>,
    pub published_block: i64,
    pub published_tx_index: i64,
    pub published_log_index: i64,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
