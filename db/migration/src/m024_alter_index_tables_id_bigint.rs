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

        match manager.get_database_backend() {
            sea_orm::DatabaseBackend::Postgres => {
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
