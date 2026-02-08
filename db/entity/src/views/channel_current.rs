//! SeaORM entity for the `channel_current` database view
//!
//! This view returns one row per channel with the latest state, using window functions
//! to select the most recent `channel_state` entry by `(published_block, published_tx_index, published_log_index)`.

use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "channel_current")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub channel_id: i64,
    pub concrete_channel_id: String,
    pub source: i64,
    pub destination: i64,
    #[sea_orm(column_type = "Binary(12)")]
    pub balance: Vec<u8>,
    pub status: i16,
    pub epoch: i64,
    pub ticket_index: i64,
    pub closure_time: Option<DateTimeWithTimeZone>,
    pub corrupted_state: bool,
    pub published_block: i64,
    pub published_tx_index: i64,
    pub published_log_index: i64,
    pub reorg_correction: bool,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}

impl From<Model> for crate::codegen::channel_state::Model {
    fn from(view: Model) -> Self {
        Self {
            id: view.id,
            channel_id: view.channel_id,
            balance: view.balance,
            status: view.status,
            epoch: view.epoch,
            ticket_index: view.ticket_index,
            closure_time: view.closure_time,
            corrupted_state: view.corrupted_state,
            published_block: view.published_block,
            published_tx_index: view.published_tx_index,
            published_log_index: view.published_log_index,
            reorg_correction: view.reorg_correction,
        }
    }
}
