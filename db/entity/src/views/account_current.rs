//! SeaORM entity for the `account_current` database view
//!
//! This view returns one row per account with the latest state, using window functions
//! to select the most recent `account_state` entry by `(published_block, published_tx_index, published_log_index)`.

use sea_orm::entity::prelude::{
    ActiveModelBehavior, DeriveEntityModel, DerivePrimaryKey, DeriveRelation, EntityTrait, EnumIter, PrimaryKeyTrait,
};

/// A row from the `account_current` database view representing the latest
/// state of a single account.
///
/// This is a read-only view model — rows are produced by the database, not
/// inserted directly. Each row corresponds to the most recent
/// `account_state` entry for a given account, selected by
/// `(published_block, published_tx_index, published_log_index)`.
///
/// # Examples
///
/// ```
/// use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};
/// # use blokli_db_entity::views::account_current;
///
/// # async fn example(db: &sea_orm::DatabaseConnection) -> Result<(), sea_orm::DbErr> {
/// let account = account_current::Entity::find()
///     .filter(account_current::Column::AccountId.eq(1_i64))
///     .one(db)
///     .await?;
///
/// if let Some(acc) = account {
///     println!("chain_key len={}, packet_key={}", acc.chain_key.len(), acc.packet_key);
/// }
/// # Ok(())
/// # }
/// ```
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

/// SeaORM relation enum for `account_current`.
///
/// Empty because this entity backs a read-only database view with no foreign
/// key relations defined at the ORM level. To resolve related data (e.g., the
/// parent account), query the relevant entity directly by key ID.
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}

impl From<Model> for crate::codegen::account_state::Model {
    fn from(view: Model) -> Self {
        Self {
            id: view.id,
            account_id: view.account_id,
            safe_address: view.safe_address,
            published_block: view.published_block,
            published_tx_index: view.published_tx_index,
            published_log_index: view.published_log_index,
        }
    }
}
