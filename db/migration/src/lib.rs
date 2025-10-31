use sea_orm_migration::async_trait;
pub use sea_orm_migration::{MigrationTrait, MigratorTrait};

mod m001_create_index_tables;
mod m002_create_index_indices;
mod m003_create_log_tables;
mod m004_create_log_indices;
mod m005_seed_initial_data;
mod m006_create_balance_and_safe_tables;
mod m007_create_performance_indices;
mod m008_add_channel_closure_grace_period;
mod m009_create_account_state_table;
mod m010_create_channel_state_table;
mod m011_alter_account_remove_safe_address;
mod m012_alter_channel_remove_mutable_fields;
mod m013_alter_chain_info_add_watermark_indices;
mod m014_add_announcement_position_index;
mod m015_create_current_state_views;
mod m016_add_channel_state_reorg_correction;

#[derive(PartialEq)]
pub enum BackendType {
    SQLite,
    Postgres,
}

pub struct Migrator;

/// Used to instantiate all tables to generate the corresponding entities in
/// a non-SQLite database (such as Postgres).
#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(m001_create_index_tables::Migration),
            Box::new(m002_create_index_indices::Migration),
            Box::new(m003_create_log_tables::Migration),
            Box::new(m004_create_log_indices::Migration),
            Box::new(m005_seed_initial_data::Migration),
            Box::new(m006_create_balance_and_safe_tables::Migration),
            Box::new(m007_create_performance_indices::Migration),
            Box::new(m008_add_channel_closure_grace_period::Migration),
            Box::new(m009_create_account_state_table::Migration),
            Box::new(m010_create_channel_state_table::Migration),
            Box::new(m011_alter_account_remove_safe_address::Migration),
            Box::new(m012_alter_channel_remove_mutable_fields::Migration),
            Box::new(m013_alter_chain_info_add_watermark_indices::Migration),
            Box::new(m014_add_announcement_position_index::Migration),
            Box::new(m015_create_current_state_views::Migration),
            Box::new(m016_add_channel_state_reorg_correction::Migration),
        ]
    }
}

/// SQLite does not allow writing lock tables only, and the write lock
/// will apply to the entire database file. It is therefore beneficial
/// to separate the exclusive concurrently accessing components into
/// separate database files to benefit from multiple write locks over
/// different parts of the database.
pub struct MigratorIndex;

#[async_trait::async_trait]
impl MigratorTrait for MigratorIndex {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(m001_create_index_tables::Migration),
            Box::new(m002_create_index_indices::Migration),
            Box::new(m005_seed_initial_data::Migration),
            Box::new(m006_create_balance_and_safe_tables::Migration),
            Box::new(m007_create_performance_indices::Migration),
            Box::new(m008_add_channel_closure_grace_period::Migration),
            Box::new(m009_create_account_state_table::Migration),
            Box::new(m010_create_channel_state_table::Migration),
            Box::new(m011_alter_account_remove_safe_address::Migration),
            Box::new(m012_alter_channel_remove_mutable_fields::Migration),
            Box::new(m013_alter_chain_info_add_watermark_indices::Migration),
            Box::new(m014_add_announcement_position_index::Migration),
            Box::new(m015_create_current_state_views::Migration),
            Box::new(m016_add_channel_state_reorg_correction::Migration),
        ]
    }
}

/// The logs are kept separate from the rest of the database to allow for
/// easier export of the logs themselves and also to not block any other database operations
/// made by the node at runtime.
pub struct MigratorChainLogs;

#[async_trait::async_trait]
impl MigratorTrait for MigratorChainLogs {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(m003_create_log_tables::Migration),
            Box::new(m004_create_log_indices::Migration),
        ]
    }
}
