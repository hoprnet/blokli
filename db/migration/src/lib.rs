pub use sea_orm_migration::prelude::*;

mod m001_create_index_tables;
mod m002_create_index_indices;
mod m003_create_log_tables;
mod m004_create_log_indices;
mod m005_seed_initial_data;

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
        ]
    }
}

/// Used to instantiate all tables for SQLite database.
/// Since SQLite cannot handle multiple concurrent writers, a separate
/// set of migrations is needed for the different database instances.
#[async_trait::async_trait]
pub trait MigratorSQLiteTrait {
    fn index_migrations() -> Vec<Box<dyn MigrationTrait>>;
    fn logs_migrations() -> Vec<Box<dyn MigrationTrait>>;
}

pub struct MigratorSQLite;

#[async_trait::async_trait]
impl MigratorSQLiteTrait for MigratorSQLite {
    fn index_migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(m001_create_index_tables::Migration),
            Box::new(m002_create_index_indices::Migration),
            Box::new(m005_seed_initial_data::Migration),
        ]
    }

    fn logs_migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(m003_create_log_tables::Migration),
            Box::new(m004_create_log_indices::Migration),
        ]
    }
}
