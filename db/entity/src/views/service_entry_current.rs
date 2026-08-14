//! SeaORM entity for the `service_entry_current` database view
//!
//! This view returns one row per service entry with the latest state, selecting the most recent
//! `service_entry_state` entry by `(published_block, published_tx_index, published_log_index)`
//! through a correlated `ORDER BY ... LIMIT 1` subquery.

use sea_orm::entity::prelude::{
    ActiveModelBehavior, DateTimeWithTimeZone, DeriveEntityModel, DerivePrimaryKey, DeriveRelation, EntityTrait,
    EnumIter, PrimaryKeyTrait,
};

/// A row from the `service_entry_current` database view representing the latest state of a single
/// service entry: one node offering one service type.
///
/// This is a read-only view model — rows are produced by the database, not inserted directly.
///
/// A row with `deregistered` set is a tombstone: the entry was removed on-chain, so
/// `safe_address`, `metadata`, `registered_at` and `updated_at` are all `None`. Callers that want
/// only live entries must filter it out.
///
/// # Examples
///
/// ```
/// use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};
/// # use blokli_db_entity::views::service_entry_current;
///
/// # async fn example(db: &sea_orm::DatabaseConnection) -> Result<(), sea_orm::DbErr> {
/// let live_entries = service_entry_current::Entity::find()
///     .filter(service_entry_current::Column::Deregistered.eq(false))
///     .all(db)
///     .await?;
///
/// println!("{} live entries", live_entries.len());
/// # Ok(())
/// # }
/// ```
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "service_entry_current")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub service_entry_id: i64,
    pub service_type_id: i64,
    #[sea_orm(column_type = "Binary(32)")]
    pub service_type: Vec<u8>,
    #[sea_orm(column_type = "Binary(20)")]
    pub node_address: Vec<u8>,
    #[sea_orm(column_type = "Binary(20)", nullable)]
    pub safe_address: Option<Vec<u8>>,
    #[sea_orm(column_type = "Binary(2048)", nullable)]
    pub metadata: Option<Vec<u8>>,
    pub registered_at: Option<DateTimeWithTimeZone>,
    pub updated_at: Option<DateTimeWithTimeZone>,
    pub deregistered: bool,
    pub published_block: i64,
    pub published_tx_index: i64,
    pub published_log_index: i64,
}

/// SeaORM relation enum for `service_entry_current`.
///
/// Empty because this entity backs a read-only database view with no foreign key relations defined
/// at the ORM level. To resolve related data, query the relevant entity directly by key ID.
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}

impl From<Model> for crate::codegen::service_entry_state::Model {
    fn from(view: Model) -> Self {
        Self {
            id: view.id,
            service_entry_id: view.service_entry_id,
            safe_address: view.safe_address,
            metadata: view.metadata,
            registered_at: view.registered_at,
            updated_at: view.updated_at,
            deregistered: view.deregistered,
            published_block: view.published_block,
            published_tx_index: view.published_tx_index,
            published_log_index: view.published_log_index,
        }
    }
}
