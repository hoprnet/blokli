use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Fix type mismatch: change id columns from INTEGER (INT4) to BIGINT (INT8)
        // to match SeaORM entity models which expect i64
        //
        // SQLite uses 64-bit INTEGER by default, so no change needed
        // PostgreSQL uses 32-bit INTEGER, needs explicit BIGINT
        //
        // This migration covers the index tables (account, channel, announcement, etc.)
        // that were not covered by m019 (chain_info, node_info) or m023 (log tables)
        //
        // NOTE: We must drop views that reference these columns before altering them,
        // then recreate the views afterward. Views were created in m015.

        match manager.get_database_backend() {
            sea_orm::DatabaseBackend::Postgres => {
                // Drop views that reference the columns we're about to alter
                manager
                    .get_connection()
                    .execute_unprepared("DROP VIEW IF EXISTS channel_current")
                    .await?;

                manager
                    .get_connection()
                    .execute_unprepared("DROP VIEW IF EXISTS account_current")
                    .await?;
                // Account table
                manager
                    .get_connection()
                    .execute_unprepared("ALTER TABLE account ALTER COLUMN id TYPE BIGINT")
                    .await?;

                // Channel table - includes foreign keys to account
                manager
                    .get_connection()
                    .execute_unprepared("ALTER TABLE channel ALTER COLUMN id TYPE BIGINT")
                    .await?;

                manager
                    .get_connection()
                    .execute_unprepared("ALTER TABLE channel ALTER COLUMN source TYPE BIGINT")
                    .await?;

                manager
                    .get_connection()
                    .execute_unprepared("ALTER TABLE channel ALTER COLUMN destination TYPE BIGINT")
                    .await?;

                // Announcement table - includes foreign key to account
                manager
                    .get_connection()
                    .execute_unprepared("ALTER TABLE announcement ALTER COLUMN id TYPE BIGINT")
                    .await?;

                manager
                    .get_connection()
                    .execute_unprepared("ALTER TABLE announcement ALTER COLUMN account_id TYPE BIGINT")
                    .await?;

                // Balance tables
                manager
                    .get_connection()
                    .execute_unprepared("ALTER TABLE hopr_balance ALTER COLUMN id TYPE BIGINT")
                    .await?;

                manager
                    .get_connection()
                    .execute_unprepared("ALTER TABLE native_balance ALTER COLUMN id TYPE BIGINT")
                    .await?;

                // Safe contract table
                manager
                    .get_connection()
                    .execute_unprepared("ALTER TABLE hopr_safe_contract ALTER COLUMN id TYPE BIGINT")
                    .await?;

                // Account state table - includes foreign key to account
                manager
                    .get_connection()
                    .execute_unprepared("ALTER TABLE account_state ALTER COLUMN id TYPE BIGINT")
                    .await?;

                manager
                    .get_connection()
                    .execute_unprepared("ALTER TABLE account_state ALTER COLUMN account_id TYPE BIGINT")
                    .await?;

                // Channel state table - includes foreign key to channel
                manager
                    .get_connection()
                    .execute_unprepared("ALTER TABLE channel_state ALTER COLUMN id TYPE BIGINT")
                    .await?;

                manager
                    .get_connection()
                    .execute_unprepared("ALTER TABLE channel_state ALTER COLUMN channel_id TYPE BIGINT")
                    .await?;

                // Recreate the views with the same definition as m015
                let create_channel_current_view = r#"
                    CREATE VIEW channel_current AS
                    SELECT
                        c.id AS channel_id,
                        c.concrete_channel_id,
                        c.source,
                        c.destination,
                        s.balance,
                        s.status,
                        s.epoch,
                        s.ticket_index,
                        s.closure_time,
                        s.corrupted_state,
                        s.published_block,
                        s.published_tx_index,
                        s.published_log_index
                    FROM channel c
                    JOIN (
                        SELECT
                            cs.*,
                            ROW_NUMBER() OVER (
                                PARTITION BY cs.channel_id
                                ORDER BY cs.published_block DESC, cs.published_tx_index DESC, cs.published_log_index DESC
                            ) AS rn
                        FROM channel_state cs
                    ) s ON s.channel_id = c.id AND s.rn = 1
                "#;

                manager
                    .get_connection()
                    .execute_unprepared(create_channel_current_view)
                    .await?;

                let create_account_current_view = r#"
                    CREATE VIEW account_current AS
                    SELECT
                        a.id AS account_id,
                        a.chain_key,
                        a.packet_key,
                        s.safe_address,
                        s.published_block,
                        s.published_tx_index,
                        s.published_log_index
                    FROM account a
                    JOIN (
                        SELECT
                            acs.*,
                            ROW_NUMBER() OVER (
                                PARTITION BY acs.account_id
                                ORDER BY acs.published_block DESC, acs.published_tx_index DESC, acs.published_log_index DESC
                            ) AS rn
                        FROM account_state acs
                    ) s ON s.account_id = a.id AND s.rn = 1
                "#;

                manager
                    .get_connection()
                    .execute_unprepared(create_account_current_view)
                    .await?;
            }
            sea_orm::DatabaseBackend::Sqlite => {
                // SQLite: INTEGER is already 64-bit, no change needed
            }
            _ => {
                // Unknown database backend, skip migration
            }
        }

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Downgrade from BIGINT back to INTEGER
        // This is safe if all IDs fit in INT4 range
        // NOTE: This reverses the alterations in reverse order to handle foreign keys

        match manager.get_database_backend() {
            sea_orm::DatabaseBackend::Postgres => {
                // Drop views before altering columns
                manager
                    .get_connection()
                    .execute_unprepared("DROP VIEW IF EXISTS channel_current")
                    .await?;

                manager
                    .get_connection()
                    .execute_unprepared("DROP VIEW IF EXISTS account_current")
                    .await?;
                // Channel state table - foreign key first
                manager
                    .get_connection()
                    .execute_unprepared("ALTER TABLE channel_state ALTER COLUMN channel_id TYPE INTEGER")
                    .await?;

                manager
                    .get_connection()
                    .execute_unprepared("ALTER TABLE channel_state ALTER COLUMN id TYPE INTEGER")
                    .await?;

                // Account state table - foreign key first
                manager
                    .get_connection()
                    .execute_unprepared("ALTER TABLE account_state ALTER COLUMN account_id TYPE INTEGER")
                    .await?;

                manager
                    .get_connection()
                    .execute_unprepared("ALTER TABLE account_state ALTER COLUMN id TYPE INTEGER")
                    .await?;

                // Safe contract table
                manager
                    .get_connection()
                    .execute_unprepared("ALTER TABLE hopr_safe_contract ALTER COLUMN id TYPE INTEGER")
                    .await?;

                // Balance tables
                manager
                    .get_connection()
                    .execute_unprepared("ALTER TABLE native_balance ALTER COLUMN id TYPE INTEGER")
                    .await?;

                manager
                    .get_connection()
                    .execute_unprepared("ALTER TABLE hopr_balance ALTER COLUMN id TYPE INTEGER")
                    .await?;

                // Announcement table - foreign key first
                manager
                    .get_connection()
                    .execute_unprepared("ALTER TABLE announcement ALTER COLUMN account_id TYPE INTEGER")
                    .await?;

                manager
                    .get_connection()
                    .execute_unprepared("ALTER TABLE announcement ALTER COLUMN id TYPE INTEGER")
                    .await?;

                // Channel table - foreign keys first
                manager
                    .get_connection()
                    .execute_unprepared("ALTER TABLE channel ALTER COLUMN destination TYPE INTEGER")
                    .await?;

                manager
                    .get_connection()
                    .execute_unprepared("ALTER TABLE channel ALTER COLUMN source TYPE INTEGER")
                    .await?;

                manager
                    .get_connection()
                    .execute_unprepared("ALTER TABLE channel ALTER COLUMN id TYPE INTEGER")
                    .await?;

                // Account table
                manager
                    .get_connection()
                    .execute_unprepared("ALTER TABLE account ALTER COLUMN id TYPE INTEGER")
                    .await?;

                // Recreate views with INTEGER columns
                let create_channel_current_view = r#"
                    CREATE VIEW channel_current AS
                    SELECT
                        c.id AS channel_id,
                        c.concrete_channel_id,
                        c.source,
                        c.destination,
                        s.balance,
                        s.status,
                        s.epoch,
                        s.ticket_index,
                        s.closure_time,
                        s.corrupted_state,
                        s.published_block,
                        s.published_tx_index,
                        s.published_log_index
                    FROM channel c
                    JOIN (
                        SELECT
                            cs.*,
                            ROW_NUMBER() OVER (
                                PARTITION BY cs.channel_id
                                ORDER BY cs.published_block DESC, cs.published_tx_index DESC, cs.published_log_index DESC
                            ) AS rn
                        FROM channel_state cs
                    ) s ON s.channel_id = c.id AND s.rn = 1
                "#;

                manager
                    .get_connection()
                    .execute_unprepared(create_channel_current_view)
                    .await?;

                let create_account_current_view = r#"
                    CREATE VIEW account_current AS
                    SELECT
                        a.id AS account_id,
                        a.chain_key,
                        a.packet_key,
                        s.safe_address,
                        s.published_block,
                        s.published_tx_index,
                        s.published_log_index
                    FROM account a
                    JOIN (
                        SELECT
                            acs.*,
                            ROW_NUMBER() OVER (
                                PARTITION BY acs.account_id
                                ORDER BY acs.published_block DESC, acs.published_tx_index DESC, acs.published_log_index DESC
                            ) AS rn
                        FROM account_state acs
                    ) s ON s.account_id = a.id AND s.rn = 1
                "#;

                manager
                    .get_connection()
                    .execute_unprepared(create_account_current_view)
                    .await?;
            }
            sea_orm::DatabaseBackend::Sqlite => {
                // SQLite: No change needed
            }
            _ => {
                // Unknown database backend, skip migration
            }
        }

        Ok(())
    }
}
