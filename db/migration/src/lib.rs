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
