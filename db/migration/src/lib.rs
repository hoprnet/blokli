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
mod m017_add_ticket_params_notify_trigger;
mod m018_alter_log_topic_info_topic_binary;
mod m019_alter_chain_node_info_id_bigint;
mod m020_add_key_binding_fee_to_chain_info;
mod m021_clear_index_data;
mod m022_clear_log_data;
mod m023_alter_log_tables_id_bigint;
mod m024_alter_index_tables_id_bigint;
mod m025_create_schema_version_table;
mod m026_add_module_and_chain_key_to_safe_contract;

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
    /// Returns the ordered set of migrations that comprise the full database migrator.
    ///
    /// This returns the complete sequence of migration instances in the order they should be applied.
    ///
    /// # Returns
    ///
    /// A `Vec<Box<dyn MigrationTrait>>` containing each migration boxed as a `MigrationTrait` object, ordered from
    /// earliest to latest.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// let migrations = Migrator::migrations();
    /// // full migrator should contain multiple migrations (at least 1)
    /// assert!(!migrations.is_empty());
    /// ```
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
            Box::new(m017_add_ticket_params_notify_trigger::Migration),
            Box::new(m018_alter_log_topic_info_topic_binary::Migration),
            Box::new(m019_alter_chain_node_info_id_bigint::Migration),
            Box::new(m020_add_key_binding_fee_to_chain_info::Migration),
            Box::new(m021_clear_index_data::Migration),
            Box::new(m022_clear_log_data::Migration),
            Box::new(m023_alter_log_tables_id_bigint::Migration),
            Box::new(m024_alter_index_tables_id_bigint::Migration),
            Box::new(m025_create_schema_version_table::Migration),
            Box::new(m026_add_module_and_chain_key_to_safe_contract::Migration),
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
    /// List of migrations to apply for the index-only migrator, in execution order.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// let list = migrations();
    /// assert!(!list.is_empty());
    /// // each element is a boxed `MigrationTrait` implementation
    /// ```
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
            Box::new(m017_add_ticket_params_notify_trigger::Migration),
            Box::new(m019_alter_chain_node_info_id_bigint::Migration),
            Box::new(m020_add_key_binding_fee_to_chain_info::Migration),
            Box::new(m021_clear_index_data::Migration),
            Box::new(m024_alter_index_tables_id_bigint::Migration),
            Box::new(m025_create_schema_version_table::Migration),
            Box::new(m026_add_module_and_chain_key_to_safe_contract::Migration),
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
            Box::new(m018_alter_log_topic_info_topic_binary::Migration),
            Box::new(m022_clear_log_data::Migration),
            Box::new(m023_alter_log_tables_id_bigint::Migration),
        ]
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
    async fn test_channel_state_performance_indices_created() {
        let db = setup_test_db().await;
        Migrator::up(&db, None).await.unwrap();

        // Verify performance indices exist
        assert!(
            index_exists(&db, "idx_channel_state_position").await,
            "idx_channel_state_position should exist"
        );

        assert!(
            index_exists(&db, "idx_channel_state_status_position").await,
            "idx_channel_state_status_position should exist"
        );

        assert!(
            index_exists(&db, "idx_channel_state_status_channel_position").await,
            "idx_channel_state_status_channel_position should exist"
        );
    }

    #[tokio::test]
    async fn test_account_state_performance_index_created() {
        let db = setup_test_db().await;
        Migrator::up(&db, None).await.unwrap();

        // Verify performance index exists
        assert!(
            index_exists(&db, "idx_account_state_position").await,
            "idx_account_state_position should exist"
        );
    }

    #[tokio::test]
    async fn test_current_state_views_created() {
        let db = setup_test_db().await;
        Migrator::up(&db, None).await.unwrap();

        // Verify views exist
        assert!(
            view_exists(&db, "channel_current").await,
            "channel_current view should exist"
        );

        assert!(
            view_exists(&db, "account_current").await,
            "account_current view should exist"
        );
    }

    #[tokio::test]
    async fn test_channel_current_view_returns_latest_state() {
        let db = setup_test_db().await;
        Migrator::up(&db, None).await.unwrap();

        // Insert test data
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

        // Insert multiple states for the same channel
        db.execute_raw(Statement::from_string(
            DbBackend::Sqlite,
            "INSERT INTO channel_state (channel_id, balance, status, epoch, ticket_index, closure_time, \
             corrupted_state, published_block, published_tx_index, published_log_index) VALUES (1, \
             X'010000000000000000000000', 1, 0, 0, NULL, 0, 100, 0, 0)"
                .to_string(),
        ))
        .await
        .unwrap();

        db.execute_raw(Statement::from_string(
            DbBackend::Sqlite,
            "INSERT INTO channel_state (channel_id, balance, status, epoch, ticket_index, closure_time, \
             corrupted_state, published_block, published_tx_index, published_log_index) VALUES (1, \
             X'020000000000000000000000', 1, 0, 0, NULL, 0, 150, 0, 0)"
                .to_string(),
        ))
        .await
        .unwrap();

        // Query view - should return latest state (block 150)
        let result = db
            .query_one_raw(Statement::from_string(
                DbBackend::Sqlite,
                "SELECT published_block FROM channel_current WHERE channel_id = 1".to_string(),
            ))
            .await
            .unwrap();

        assert!(result.is_some(), "View should return result");
        let row = result.unwrap();
        let block: i64 = row.try_get("", "published_block").unwrap();
        assert_eq!(block, 150, "View should return latest state at block 150");
    }

    #[tokio::test]
    async fn test_account_current_view_returns_latest_state() {
        let db = setup_test_db().await;
        Migrator::up(&db, None).await.unwrap();

        // Insert test data
        db.execute_raw(Statement::from_string(
            DbBackend::Sqlite,
            "INSERT INTO account (chain_key, packet_key) VALUES (X'0101010101010101010101010101010101010101', 'peer1')"
                .to_string(),
        ))
        .await
        .unwrap();

        // Insert multiple states for the same account
        db.execute_raw(Statement::from_string(
            DbBackend::Sqlite,
            "INSERT INTO account_state (account_id, safe_address, published_block, published_tx_index, \
             published_log_index) VALUES (1, X'0202020202020202020202020202020202020202', 100, 0, 0)"
                .to_string(),
        ))
        .await
        .unwrap();

        db.execute_raw(Statement::from_string(
            DbBackend::Sqlite,
            "INSERT INTO account_state (account_id, safe_address, published_block, published_tx_index, \
             published_log_index) VALUES (1, X'0303030303030303030303030303030303030303', 150, 0, 0)"
                .to_string(),
        ))
        .await
        .unwrap();

        // Query view - should return latest state (block 150)
        let result = db
            .query_one_raw(Statement::from_string(
                DbBackend::Sqlite,
                "SELECT published_block FROM account_current WHERE account_id = 1".to_string(),
            ))
            .await
            .unwrap();

        assert!(result.is_some(), "View should return result");
        let row = result.unwrap();
        let block: i64 = row.try_get("", "published_block").unwrap();
        assert_eq!(block, 150, "View should return latest state at block 150");
    }

    #[tokio::test]
    async fn test_foreign_key_cascade_on_account_state() {
        let db = setup_test_db().await;
        Migrator::up(&db, None).await.unwrap();

        // Insert account and state
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
             published_log_index) VALUES (1, NULL, 100, 0, 0)"
                .to_string(),
        ))
        .await
        .unwrap();

        // Verify state exists
        let result_before = db
            .query_one_raw(Statement::from_string(
                DbBackend::Sqlite,
                "SELECT COUNT(*) as cnt FROM account_state WHERE account_id = 1".to_string(),
            ))
            .await
            .unwrap()
            .unwrap();
        let count_before: i32 = result_before.try_get("", "cnt").unwrap();
        assert_eq!(count_before, 1, "Should have 1 account_state record");

        // Delete account - should cascade to account_state
        db.execute_raw(Statement::from_string(
            DbBackend::Sqlite,
            "DELETE FROM account WHERE id = 1".to_string(),
        ))
        .await
        .unwrap();

        // Verify state was also deleted
        let result_after = db
            .query_one_raw(Statement::from_string(
                DbBackend::Sqlite,
                "SELECT COUNT(*) as cnt FROM account_state WHERE account_id = 1".to_string(),
            ))
            .await
            .unwrap()
            .unwrap();
        let count_after: i32 = result_after.try_get("", "cnt").unwrap();
        assert_eq!(count_after, 0, "account_state should be deleted via cascade");
    }

    #[tokio::test]
    async fn test_foreign_key_cascade_on_channel_state() {
        let db = setup_test_db().await;
        Migrator::up(&db, None).await.unwrap();

        // Insert accounts and channel
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
             X'010000000000000000000000', 1, 0, 0, NULL, 0, 100, 0, 0)"
                .to_string(),
        ))
        .await
        .unwrap();

        // Verify state exists
        let result_before = db
            .query_one_raw(Statement::from_string(
                DbBackend::Sqlite,
                "SELECT COUNT(*) as cnt FROM channel_state WHERE channel_id = 1".to_string(),
            ))
            .await
            .unwrap()
            .unwrap();
        let count_before: i32 = result_before.try_get("", "cnt").unwrap();
        assert_eq!(count_before, 1, "Should have 1 channel_state record");

        // Delete channel - should cascade to channel_state
        db.execute_raw(Statement::from_string(
            DbBackend::Sqlite,
            "DELETE FROM channel WHERE id = 1".to_string(),
        ))
        .await
        .unwrap();

        // Verify state was also deleted
        let result_after = db
            .query_one_raw(Statement::from_string(
                DbBackend::Sqlite,
                "SELECT COUNT(*) as cnt FROM channel_state WHERE channel_id = 1".to_string(),
            ))
            .await
            .unwrap()
            .unwrap();
        let count_after: i32 = result_after.try_get("", "cnt").unwrap();
        assert_eq!(count_after, 0, "channel_state should be deleted via cascade");
    }

    #[tokio::test]
    async fn test_chain_info_watermark_indices_exist() {
        let db = setup_test_db().await;
        Migrator::up(&db, None).await.unwrap();

        // Verify chain_info table exists with watermark fields
        let insert_result = db
            .execute_raw(Statement::from_string(
                DbBackend::Sqlite,
                "INSERT INTO chain_info (last_indexed_block, last_indexed_tx_index, last_indexed_log_index, \
                 min_incoming_ticket_win_prob) VALUES (100, 5, 3, 0.5)"
                    .to_string(),
            ))
            .await;

        assert!(
            insert_result.is_ok(),
            "Should be able to insert into chain_info with watermark fields"
        );
    }
}
