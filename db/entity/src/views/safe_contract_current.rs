//! SeaORM entity for the `safe_contract_current` database view
//!
//! This view returns one row per safe contract with the latest state, selecting
//! the most recent `hopr_safe_contract_state` entry by
//! `(published_block, published_tx_index, published_log_index)`.

use sea_orm::entity::prelude::{
    ActiveModelBehavior, DeriveEntityModel, DerivePrimaryKey, DeriveRelation, EntityTrait, EnumIter, PrimaryKeyTrait,
};

/// A row from the `safe_contract_current` database view representing the latest
/// state of a single safe contract.
///
/// This is a read-only view model — rows are produced by the database, not
/// inserted directly. Each row corresponds to the most recent
/// `hopr_safe_contract_state` entry for a given safe contract.
///
/// # Examples
///
/// ```
/// use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};
/// # use blokli_db_entity::views::safe_contract_current;
///
/// # async fn example(db: &sea_orm::DatabaseConnection) -> Result<(), sea_orm::DbErr> {
/// let safe = safe_contract_current::Entity::find()
///     .filter(safe_contract_current::Column::ChainKey.eq(vec![0u8; 20]))
///     .one(db)
///     .await?;
///
/// if let Some(s) = safe {
///     println!("address len={}", s.address.len());
/// }
/// # Ok(())
/// # }
/// ```
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "safe_contract_current")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub safe_contract_id: i64,
    #[sea_orm(column_type = "Binary(20)")]
    pub address: Vec<u8>,
    #[sea_orm(column_type = "Binary(20)")]
    pub module_address: Vec<u8>,
    #[sea_orm(column_type = "Binary(20)")]
    pub chain_key: Vec<u8>,
    pub published_block: i64,
    pub published_tx_index: i64,
    pub published_log_index: i64,
}

/// SeaORM relation enum for `safe_contract_current`.
///
/// Empty because this entity backs a read-only database view with no foreign
/// key relations defined at the ORM level.
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
