use sea_orm_migration::async_trait;
pub use sea_orm_migration::{MigrationTrait, MigratorTrait};

mod m001_initial_schema;
mod m002_initial_log_schema;
mod m003_safe_history_schema;
mod m004_safe_redeemed_stats_rejections;

/// This is a special block ID that even pre-dates the v3 contract deployment on Gnosis chain,
/// and therefore could be safely used to mark data added via the migration.
///
/// This allows distinguishing between data added via the migration and data added via other means.
/// The data added via migration are e.g.: not cleared.
pub const MIGRATION_MARKER_BLOCK_ID: u32 = 1000;

#[derive(PartialEq)]
pub enum BackendType {
    SQLite,
    Postgres,
}

/// Indicates from which network the v3 Safe data should be imported.
#[repr(u8)]
pub enum SafeDataOrigin {
    /// Do not import any v3 Safe data.
    NoData = 0,
    /// Import v3 Jura Safe data.
    Jura = 1,
}

/// Contains all migrations for non-SQLite databases (e.g. Postgres) and also
/// for SQLite when a single unified database file is used (no separate logs DB).
pub struct Migrator<const NETWORK: u8>;

impl<const NETWORK: u8> Migrator<NETWORK> {
    fn base_migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(m001_initial_schema::Migration),
            Box::new(m002_initial_log_schema::Migration),
            Box::new(m003_safe_history_schema::Migration),
            Box::new(m004_safe_redeemed_stats_rejections::Migration),
        ]
    }
}

#[async_trait::async_trait]
impl MigratorTrait for Migrator<{ SafeDataOrigin::NoData as u8 }> {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        Self::base_migrations()
    }
}

#[async_trait::async_trait]
impl MigratorTrait for Migrator<{ SafeDataOrigin::Jura as u8 }> {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        Self::base_migrations()
    }
}

/// SQLite does not allow writing lock tables only, and the write lock
/// will apply to the entire database file. It is therefore beneficial
/// to place components that need concurrent exclusive write access into
/// separate database files so that multiple write locks can be used over
/// different parts of the database.
pub struct MigratorIndex<const NETWORK: u8>;

impl<const NETWORK: u8> MigratorIndex<NETWORK> {
    fn base_migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(m001_initial_schema::Migration),
            Box::new(m003_safe_history_schema::Migration),
            Box::new(m004_safe_redeemed_stats_rejections::Migration),
        ]
    }
}

#[async_trait::async_trait]
impl MigratorTrait for MigratorIndex<{ SafeDataOrigin::NoData as u8 }> {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        Self::base_migrations()
    }
}

#[async_trait::async_trait]
impl MigratorTrait for MigratorIndex<{ SafeDataOrigin::Jura as u8 }> {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        Self::base_migrations()
    }
}

/// The logs are kept separate from the rest of the database to allow for
/// easier export of the logs themselves and also to not block any other database operations
/// made by the node at runtime.
pub struct MigratorChainLogs;

#[async_trait::async_trait]
impl MigratorTrait for MigratorChainLogs {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![Box::new(m002_initial_log_schema::Migration)]
    }
}

#[cfg(test)]
mod tests {
    use sea_orm::{ConnectionTrait, Database, DatabaseConnection, DbBackend, Statement};

    use super::*;

    async fn setup_test_db() -> DatabaseConnection {
        // Create in-memory SQLite database for testing
        Database::connect("sqlite::memory:")
            .await
            .expect("Failed to create test database")
    }

    async fn table_exists(db: &DatabaseConnection, table_name: &str) -> bool {
        let stmt = Statement::from_string(
            DbBackend::Sqlite,
            format!(
                "SELECT name FROM sqlite_master WHERE type='table' AND name='{}'",
                table_name
            ),
        );
        let result = db.query_one_raw(stmt).await.expect("Failed to query table existence");

        result.is_some()
    }

    async fn view_exists(db: &DatabaseConnection, view_name: &str) -> bool {
        let stmt = Statement::from_string(
            DbBackend::Sqlite,
            format!(
                "SELECT name FROM sqlite_master WHERE type='view' AND name='{}'",
                view_name
            ),
        );
        let result = db.query_one_raw(stmt).await.expect("Failed to query view existence");

        result.is_some()
    }

    async fn index_exists(db: &DatabaseConnection, index_name: &str) -> bool {
        let stmt = Statement::from_string(
            DbBackend::Sqlite,
            format!(
                "SELECT name FROM sqlite_master WHERE type='index' AND name='{}'",
                index_name
            ),
        );
        let result = db.query_one_raw(stmt).await.expect("Failed to query index existence");

        result.is_some()
    }

    #[tokio::test]
    async fn test_all_migrations_run_successfully() {
        let db = setup_test_db().await;

        // Run all migrations
        let result = Migrator::<{ SafeDataOrigin::NoData as u8 }>::up(&db, None).await;

        assert!(result.is_ok(), "Migrations should run without errors");
    }

    #[tokio::test]
    async fn test_account_state_table_created() {
        let db = setup_test_db().await;
        Migrator::<{ SafeDataOrigin::NoData as u8 }>::up(&db, None)
            .await
            .unwrap();

        // Verify account_state table exists
        assert!(
            table_exists(&db, "account_state").await,
            "account_state table should exist"
        );

        // Verify table structure by inserting and querying
        let insert_result = db
            .execute_raw(Statement::from_string(
                DbBackend::Sqlite,
                "INSERT INTO account (chain_key, packet_key) VALUES (X'0101010101010101010101010101010101010101', \
                 'peer1')"
                    .to_string(),
            ))
            .await;
        assert!(insert_result.is_ok(), "Should be able to insert into account");

        let insert_state_result = db
            .execute_raw(Statement::from_string(
                DbBackend::Sqlite,
                "INSERT INTO account_state (account_id, safe_address, published_block, published_tx_index, \
                 published_log_index) VALUES (1, X'0202020202020202020202020202020202020202', 100, 5, 3)"
                    .to_string(),
            ))
            .await;
        assert!(
            insert_state_result.is_ok(),
            "Should be able to insert into account_state"
        );
    }

    #[tokio::test]
    async fn test_channel_state_table_created() {
        let db = setup_test_db().await;
        Migrator::<{ SafeDataOrigin::NoData as u8 }>::up(&db, None)
            .await
            .unwrap();

        // Verify channel_state table exists
        assert!(
            table_exists(&db, "channel_state").await,
            "channel_state table should exist"
        );

        // Verify table structure by inserting data
        // First insert accounts
        db.execute_raw(Statement::from_string(
            DbBackend::Sqlite,
            "INSERT INTO account (chain_key, packet_key) VALUES (X'0101010101010101010101010101010101010101', 'peer1')"
                .to_string(),
        ))
        .await
        .unwrap();

        db.execute_raw(Statement::from_string(
            DbBackend::Sqlite,
            "INSERT INTO account (chain_key, packet_key) VALUES (X'0202020202020202020202020202020202020202', 'peer2')"
                .to_string(),
        ))
        .await
        .unwrap();

        // Insert channel
        db.execute_raw(Statement::from_string(
            DbBackend::Sqlite,
            "INSERT INTO channel (source, destination, concrete_channel_id) VALUES (1, 2, '0xabc123')".to_string(),
        ))
        .await
        .unwrap();

        // Insert channel_state
        let insert_result = db
            .execute_raw(Statement::from_string(
                DbBackend::Sqlite,
                "INSERT INTO channel_state (channel_id, balance, status, epoch, ticket_index, closure_time, \
                 corrupted_state, published_block, published_tx_index, published_log_index) VALUES (1, \
                 X'010000000000000000000000', 1, 0, 0, NULL, 0, 100, 5, 3)"
                    .to_string(),
            ))
            .await;

        assert!(insert_result.is_ok(), "Should be able to insert into channel_state");
    }

    #[tokio::test]
    async fn test_account_state_unique_position_index_created() {
        let db = setup_test_db().await;
        Migrator::<{ SafeDataOrigin::NoData as u8 }>::up(&db, None)
            .await
            .unwrap();

        // Verify unique index exists
        assert!(
            index_exists(&db, "idx_account_state_unique_position").await,
            "idx_account_state_unique_position should exist"
        );

        // Test uniqueness constraint
        db.execute_raw(Statement::from_string(
            DbBackend::Sqlite,
            "INSERT INTO account (chain_key, packet_key) VALUES (X'0101010101010101010101010101010101010101', 'peer1')"
                .to_string(),
        ))
        .await
        .unwrap();

        db.execute_raw(Statement::from_string(
            DbBackend::Sqlite,
            "INSERT INTO account_state (account_id, safe_address, published_block, published_tx_index, \
             published_log_index) VALUES (1, NULL, 100, 5, 3)"
                .to_string(),
        ))
        .await
        .unwrap();

        // Try to insert duplicate - should fail
        let duplicate_result = db
            .execute_raw(Statement::from_string(
                DbBackend::Sqlite,
                "INSERT INTO account_state (account_id, safe_address, published_block, published_tx_index, \
                 published_log_index) VALUES (1, NULL, 100, 5, 3)"
                    .to_string(),
            ))
            .await;

        assert!(
            duplicate_result.is_err(),
            "Duplicate position should be rejected by unique constraint"
        );
    }

    #[tokio::test]
    async fn test_channel_state_unique_position_index_created() {
        let db = setup_test_db().await;
        Migrator::<{ SafeDataOrigin::NoData as u8 }>::up(&db, None)
            .await
            .unwrap();

        // Verify unique index exists
        assert!(
            index_exists(&db, "idx_channel_state_unique_position").await,
            "idx_channel_state_unique_position should exist"
        );

        // Test uniqueness constraint
        db.execute_raw(Statement::from_string(
            DbBackend::Sqlite,
            "INSERT INTO account (chain_key, packet_key) VALUES (X'0101010101010101010101010101010101010101', 'peer1')"
                .to_string(),
        ))
        .await
        .unwrap();

        db.execute_raw(Statement::from_string(
            DbBackend::Sqlite,
            "INSERT INTO account (chain_key, packet_key) VALUES (X'0202020202020202020202020202020202020202', 'peer2')"
                .to_string(),
        ))
        .await
        .unwrap();

        db.execute_raw(Statement::from_string(
            DbBackend::Sqlite,
            "INSERT INTO channel (source, destination, concrete_channel_id) VALUES (1, 2, '0xabc')".to_string(),
        ))
        .await
        .unwrap();

        db.execute_raw(Statement::from_string(
            DbBackend::Sqlite,
            "INSERT INTO channel_state (channel_id, balance, status, epoch, ticket_index, closure_time, \
             corrupted_state, published_block, published_tx_index, published_log_index) VALUES (1, \
             X'010000000000000000000000', 1, 0, 0, NULL, 0, 100, 5, 3)"
                .to_string(),
        ))
        .await
        .unwrap();

        // Try to insert duplicate - should fail
        let duplicate_result = db
            .execute_raw(Statement::from_string(
                DbBackend::Sqlite,
                "INSERT INTO channel_state (channel_id, balance, status, epoch, ticket_index, closure_time, \
                 corrupted_state, published_block, published_tx_index, published_log_index) VALUES (1, \
                 X'010000000000000000000000', 1, 0, 0, NULL, 0, 100, 5, 3)"
                    .to_string(),
            ))
            .await;

        assert!(
            duplicate_result.is_err(),
            "Duplicate position should be rejected by unique constraint"
        );
    }

    #[tokio::test]
    async fn test_views_created() {
        let db = setup_test_db().await;
        Migrator::<{ SafeDataOrigin::NoData as u8 }>::up(&db, None)
            .await
            .unwrap();

        assert!(
            view_exists(&db, "channel_current").await,
            "channel_current view should exist"
        );
        assert!(
            view_exists(&db, "account_current").await,
            "account_current view should exist"
        );
        assert!(
            view_exists(&db, "safe_contract_current").await,
            "safe_contract_current view should exist"
        );
    }
}
