//! SeaORM entity for the `safe_threshold_current` database view

use sea_orm::entity::prelude::{
    ActiveModelBehavior, DeriveEntityModel, DerivePrimaryKey, DeriveRelation, EntityTrait, EnumIter, PrimaryKeyTrait,
};

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "safe_threshold_current")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub safe_contract_id: i64,
    #[sea_orm(column_type = "Binary(20)")]
    pub safe_address: Vec<u8>,
    pub threshold: String,
    pub published_block: i64,
    pub published_tx_index: i64,
    pub published_log_index: i64,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
