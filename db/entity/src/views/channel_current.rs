//! SeaORM entity for the `channel_current` database view
//!
//! This view returns one row per channel with the latest state, using window functions
//! to select the most recent `channel_state` entry by `(published_block, published_tx_index, published_log_index)`.

use sea_orm::entity::prelude::{
    ActiveModelBehavior, DateTimeWithTimeZone, DeriveEntityModel, DerivePrimaryKey, DeriveRelation, EntityTrait,
    EnumIter, PrimaryKeyTrait,
};

/// A row from the `channel_current` database view representing the latest
/// state of a single channel.
///
/// This is a read-only view model — rows are produced by the database, not
/// inserted directly. Each row corresponds to the most recent
/// `channel_state` entry for a given channel, selected by
/// `(published_block, published_tx_index, published_log_index)`.
///
/// # Examples
///
/// ```
/// use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};
/// # use blokli_db_entity::views::channel_current;
///
/// # async fn example(db: &sea_orm::DatabaseConnection) -> Result<(), sea_orm::DbErr> {
/// let channel = channel_current::Entity::find()
///     .filter(channel_current::Column::ChannelId.eq(42_i64))
///     .one(db)
///     .await?;
///
/// if let Some(ch) = channel {
///     println!("status={}, epoch={}", ch.status, ch.epoch);
/// }
/// # Ok(())
/// # }
/// ```
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

/// SeaORM relation enum for `channel_current`.
///
/// Empty because this entity backs a read-only database view with no foreign
/// key relations defined at the ORM level. To resolve related data (e.g., the
/// source or destination account), query the relevant entity directly by key ID.
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
