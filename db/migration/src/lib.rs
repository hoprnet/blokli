use sea_orm_migration::async_trait;
pub use sea_orm_migration::{MigrationTrait, MigratorTrait};

mod m001_initial_schema;
mod m002_initial_log_schema;
mod m003_safe_history_schema;
mod m004_safe_redeemed_stats_rejections;
mod m005_optimize_current_views;
mod m006_service_registry_schema;
mod m007_curvy_note_tree;

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

/// Contains all migrations for non-SQLite databases (e.g. Postgres) and also
/// for SQLite when a single unified database file is used (no separate logs DB).
pub struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(m001_initial_schema::Migration),
            Box::new(m002_initial_log_schema::Migration),
            Box::new(m003_safe_history_schema::Migration),
            Box::new(m004_safe_redeemed_stats_rejections::Migration),
            Box::new(m005_optimize_current_views::Migration),
            Box::new(m006_service_registry_schema::Migration),
            Box::new(m007_curvy_note_tree::Migration),
        ]
    }
}

/// SQLite does not allow writing lock tables only, and the write lock
/// will apply to the entire database file. It is therefore beneficial
/// to place components that need concurrent exclusive write access into
/// separate database files so that multiple write locks can be used over
/// different parts of the database.
pub struct MigratorIndex;

#[async_trait::async_trait]
impl MigratorTrait for MigratorIndex {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(m001_initial_schema::Migration),
            Box::new(m003_safe_history_schema::Migration),
            Box::new(m004_safe_redeemed_stats_rejections::Migration),
            Box::new(m005_optimize_current_views::Migration),
            Box::new(m006_service_registry_schema::Migration),
            Box::new(m007_curvy_note_tree::Migration),
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
        let result = Migrator::up(&db, None).await;

        assert!(result.is_ok(), "Migrations should run without errors");
    }

    #[tokio::test]
    async fn test_account_state_table_created() {
        let db = setup_test_db().await;
        Migrator::up(&db, None).await.unwrap();

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
        Migrator::up(&db, None).await.unwrap();

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
        Migrator::up(&db, None).await.unwrap();

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
        Migrator::up(&db, None).await.unwrap();

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
        Migrator::up(&db, None).await.unwrap();

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

    /// A 32-byte service type id, right-padded ASCII `gvpn:exit`.
    const SERVICE_TYPE_HEX: &str = "X'6776706e3a657869740000000000000000000000000000000000000000000000'";
    /// A 32-byte burn amount of one wei.
    const BURN_HEX: &str = "X'0000000000000000000000000000000000000000000000000000000000000001'";
    const NODE_HEX: &str = "X'0303030303030303030303030303030303030303'";
    const SAFE_HEX: &str = "X'0404040404040404040404040404040404040404'";
    const OWNER_HEX: &str = "X'0505050505050505050505050505050505050505'";

    /// Runs the whole migration stack and seeds one service type with one entry, both with a
    /// single state row at position `(100, 0, 0)` / `(100, 0, 1)`.
    async fn setup_seeded_service_registry() -> DatabaseConnection {
        let db = setup_test_db().await;
        Migrator::up(&db, None).await.unwrap();

        db.execute_raw(Statement::from_string(
            DbBackend::Sqlite,
            format!("INSERT INTO service_type (service_type) VALUES ({SERVICE_TYPE_HEX})"),
        ))
        .await
        .expect("should be able to insert into service_type");

        db.execute_raw(Statement::from_string(
            DbBackend::Sqlite,
            format!(
                "INSERT INTO service_type_state (service_type_id, owner_address, requirement_address, \
                 registration_burn, update_burn, published_block, published_tx_index, published_log_index) VALUES (1, \
                 {OWNER_HEX}, NULL, {BURN_HEX}, {BURN_HEX}, 100, 0, 0)"
            ),
        ))
        .await
        .expect("should be able to insert into service_type_state");

        db.execute_raw(Statement::from_string(
            DbBackend::Sqlite,
            format!("INSERT INTO service_entry (service_type_id, node_address) VALUES (1, {NODE_HEX})"),
        ))
        .await
        .expect("should be able to insert into service_entry");

        db.execute_raw(Statement::from_string(
            DbBackend::Sqlite,
            format!(
                "INSERT INTO service_entry_state (service_entry_id, safe_address, metadata, registered_at, \
                 updated_at, deregistered, published_block, published_tx_index, published_log_index) VALUES (1, \
                 {SAFE_HEX}, X'0102', '2024-01-01 00:00:00+00:00', '2024-01-01 00:00:00+00:00', 0, 100, 0, 1)"
            ),
        ))
        .await
        .expect("should be able to insert into service_entry_state");

        db
    }

    #[tokio::test]
    async fn test_service_registry_tables_created() {
        let db = setup_seeded_service_registry().await;

        for table in [
            "service_type",
            "service_type_state",
            "service_entry",
            "service_entry_state",
            "service_registry_config",
        ] {
            assert!(table_exists(&db, table).await, "{table} table should exist");
        }
    }

    #[tokio::test]
    async fn test_service_registry_views_created() {
        let db = setup_seeded_service_registry().await;

        assert!(
            view_exists(&db, "service_type_current").await,
            "service_type_current view should exist"
        );
        assert!(
            view_exists(&db, "service_entry_current").await,
            "service_entry_current view should exist"
        );
    }

    /// The `*_current` views must project the newest state row, so a later state row supersedes
    /// the seeded one.
    #[tokio::test]
    async fn test_service_current_views_return_latest_state() {
        let db = setup_seeded_service_registry().await;

        db.execute_raw(Statement::from_string(
            DbBackend::Sqlite,
            format!(
                "INSERT INTO service_type_state (service_type_id, owner_address, requirement_address, \
                 registration_burn, update_burn, published_block, published_tx_index, published_log_index) VALUES (1, \
                 NULL, NULL, {BURN_HEX}, {BURN_HEX}, 200, 0, 0)"
            ),
        ))
        .await
        .unwrap();

        db.execute_raw(Statement::from_string(
            DbBackend::Sqlite,
            "INSERT INTO service_entry_state (service_entry_id, safe_address, metadata, registered_at, updated_at, \
             deregistered, published_block, published_tx_index, published_log_index) VALUES (1, NULL, NULL, NULL, \
             NULL, 1, 200, 0, 1)"
                .to_string(),
        ))
        .await
        .unwrap();

        let type_row = db
            .query_one_raw(Statement::from_string(
                DbBackend::Sqlite,
                "SELECT published_block, owner_address FROM service_type_current".to_string(),
            ))
            .await
            .unwrap()
            .expect("service_type_current should return a row");
        assert_eq!(type_row.try_get::<i64>("", "published_block").unwrap(), 200);
        assert_eq!(type_row.try_get::<Option<Vec<u8>>>("", "owner_address").unwrap(), None);

        let entry_row = db
            .query_one_raw(Statement::from_string(
                DbBackend::Sqlite,
                "SELECT published_block, deregistered FROM service_entry_current".to_string(),
            ))
            .await
            .unwrap()
            .expect("service_entry_current should return a row");
        assert_eq!(entry_row.try_get::<i64>("", "published_block").unwrap(), 200);
        assert!(entry_row.try_get::<bool>("", "deregistered").unwrap());
    }

    #[tokio::test]
    async fn test_service_state_position_indexes_created() {
        let db = setup_seeded_service_registry().await;

        for index in [
            "idx_service_type_state_unique_position",
            "idx_service_type_state_position",
            "idx_service_entry_state_unique_position",
            "idx_service_entry_state_position",
        ] {
            assert!(index_exists(&db, index).await, "{index} should exist");
        }

        // Replaying the same log position must be rejected by the unique indexes.
        let duplicate_type_state = db
            .execute_raw(Statement::from_string(
                DbBackend::Sqlite,
                format!(
                    "INSERT INTO service_type_state (service_type_id, owner_address, requirement_address, \
                     registration_burn, update_burn, published_block, published_tx_index, published_log_index) VALUES \
                     (1, {OWNER_HEX}, NULL, {BURN_HEX}, {BURN_HEX}, 100, 0, 0)"
                ),
            ))
            .await;
        assert!(
            duplicate_type_state.is_err(),
            "Duplicate service_type_state position should be rejected"
        );

        let duplicate_entry_state = db
            .execute_raw(Statement::from_string(
                DbBackend::Sqlite,
                format!(
                    "INSERT INTO service_entry_state (service_entry_id, safe_address, metadata, registered_at, \
                     updated_at, deregistered, published_block, published_tx_index, published_log_index) VALUES (1, \
                     {SAFE_HEX}, X'0102', '2024-01-01 00:00:00+00:00', '2024-01-01 00:00:00+00:00', 0, 100, 0, 1)"
                ),
            ))
            .await;
        assert!(
            duplicate_entry_state.is_err(),
            "Duplicate service_entry_state position should be rejected"
        );
    }

    #[tokio::test]
    async fn test_service_entry_unique_type_node_index() {
        let db = setup_seeded_service_registry().await;

        assert!(
            index_exists(&db, "idx_service_entry_unique_type_node").await,
            "idx_service_entry_unique_type_node should exist"
        );

        let duplicate = db
            .execute_raw(Statement::from_string(
                DbBackend::Sqlite,
                format!("INSERT INTO service_entry (service_type_id, node_address) VALUES (1, {NODE_HEX})"),
            ))
            .await;
        assert!(
            duplicate.is_err(),
            "Duplicate (service_type_id, node_address) pair should be rejected"
        );
    }

    /// A service entry must be storable for a node that has no `account` row: the registry does
    /// not require an announcement, so the node is a raw address rather than a foreign key.
    #[tokio::test]
    async fn test_service_entry_node_needs_no_account() {
        let db = setup_seeded_service_registry().await;

        let account_count = db
            .query_one_raw(Statement::from_string(
                DbBackend::Sqlite,
                "SELECT COUNT(*) AS n FROM account".to_string(),
            ))
            .await
            .unwrap()
            .expect("count query should return a row");
        assert_eq!(account_count.try_get::<i64>("", "n").unwrap(), 0);

        let entry_count = db
            .query_one_raw(Statement::from_string(
                DbBackend::Sqlite,
                "SELECT COUNT(*) AS n FROM service_entry".to_string(),
            ))
            .await
            .unwrap()
            .expect("count query should return a row");
        assert_eq!(entry_count.try_get::<i64>("", "n").unwrap(), 1);
    }

    #[tokio::test]
    async fn test_service_registry_config_is_singleton() {
        let db = setup_seeded_service_registry().await;

        db.execute_raw(Statement::from_string(
            DbBackend::Sqlite,
            format!(
                "INSERT INTO service_registry_config (id, type_registration_fee, node_safe_registry) VALUES (1, \
                 {BURN_HEX}, {SAFE_HEX})"
            ),
        ))
        .await
        .expect("should be able to insert the singleton config row");

        let duplicate = db
            .execute_raw(Statement::from_string(
                DbBackend::Sqlite,
                format!(
                    "INSERT INTO service_registry_config (id, type_registration_fee, node_safe_registry) VALUES (1, \
                     {BURN_HEX}, NULL)"
                ),
            ))
            .await;
        assert!(duplicate.is_err(), "service_registry_config must hold a single row");
    }
}
