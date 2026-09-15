//! SeaORM entity for the `service_type_current` database view
//!
//! This view returns one row per service type with the latest state, selecting the most recent
//! `service_type_state` entry by `(published_block, published_tx_index, published_log_index)`
//! through a correlated `ORDER BY ... LIMIT 1` subquery.

use sea_orm::entity::prelude::{
    ActiveModelBehavior, DeriveEntityModel, DerivePrimaryKey, DeriveRelation, EntityTrait, EnumIter, PrimaryKeyTrait,
};

/// A row from the `service_type_current` database view representing the latest state of a single
/// service type.
///
/// This is a read-only view model — rows are produced by the database, not inserted directly.
///
/// A `None` `owner_address` means the type was abandoned, and a `None` `requirement_address` means
/// the type is open to any node; both encode the contract's zero-address sentinels.
///
/// # Examples
///
/// ```
/// use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};
/// # use blokli_db_entity::views::service_type_current;
///
/// # async fn example(db: &sea_orm::DatabaseConnection) -> Result<(), sea_orm::DbErr> {
/// let service_type = service_type_current::Entity::find()
///     .filter(service_type_current::Column::ServiceTypeId.eq(1_i64))
///     .one(db)
///     .await?;
///
/// if let Some(st) = service_type {
///     println!("open type: {}", st.requirement_address.is_none());
/// }
/// # Ok(())
/// # }
/// ```
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "service_type_current")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub service_type_id: i64,
    #[sea_orm(column_type = "Binary(32)")]
    pub service_type: Vec<u8>,
    #[sea_orm(column_type = "Binary(20)", nullable)]
    pub owner_address: Option<Vec<u8>>,
    #[sea_orm(column_type = "Binary(20)", nullable)]
    pub requirement_address: Option<Vec<u8>>,
    #[sea_orm(column_type = "Binary(32)")]
    pub registration_burn: Vec<u8>,
    #[sea_orm(column_type = "Binary(32)")]
    pub update_burn: Vec<u8>,
    pub published_block: i64,
    pub published_tx_index: i64,
    pub published_log_index: i64,
}

/// SeaORM relation enum for `service_type_current`.
///
/// Empty because this entity backs a read-only database view with no foreign key relations defined
/// at the ORM level. To resolve related data, query the relevant entity directly by key ID.
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}

impl From<Model> for crate::codegen::service_type_state::Model {
    fn from(view: Model) -> Self {
        Self {
            id: view.id,
            service_type_id: view.service_type_id,
            owner_address: view.owner_address,
            requirement_address: view.requirement_address,
            registration_burn: view.registration_burn,
            update_burn: view.update_burn,
            published_block: view.published_block,
            published_tx_index: view.published_tx_index,
            published_log_index: view.published_log_index,
        }
    }
}
