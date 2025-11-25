use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

/// WARNING: This migration is ONLY for fresh databases with no existing data.
///
/// This migration converts the LogTopicInfo.topic column from varchar(64) to binary(32)
/// to properly store 32-byte hash values as binary data instead of hex strings.
///
/// BREAKING CHANGE: This migration drops and recreates the LogTopicInfo table, which
/// will DELETE ALL EXISTING ROWS. This is acceptable only because:
/// 1. This migration was introduced before any production deployments
/// 2. The database schema is still in active development
/// 3. Existing development/test databases can be recreated from chain data
///
/// If you have an existing database with data you need to preserve, DO NOT run this
/// migration. Instead, either:
/// - Re-index from scratch (recommended for development)
/// - Manually migrate data using custom SQL that converts hex strings to binary
///
/// For future migrations on production systems, data preservation MUST be implemented
/// using temporary tables and data transformation.
#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Change topic column from varchar(64) to binary(32)
        // Hash values are 32 bytes and should be stored as binary, not hex strings
        //
        // Both PostgreSQL and SQLite need to recreate the table because:
        // - SQLite doesn't support ALTER TABLE MODIFY COLUMN
        // - PostgreSQL can't automatically cast varchar to bytea
        //
        // DESTRUCTIVE: This drops existing data. Only safe for fresh databases.

        // Drop the old index first (ignore error if it doesn't exist)
        let _ = manager
            .drop_index(
                Index::drop()
                    .name("idx_contract_log_topic")
                    .table(LogTopicInfo::Table)
                    .to_owned(),
            )
            .await;

        // Drop the old table
        manager
            .drop_table(Table::drop().table(LogTopicInfo::Table).to_owned())
            .await?;

        // Create new table with binary topic column
        manager
            .create_table(
                Table::create()
                    .table(LogTopicInfo::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(LogTopicInfo::Id)
                            .primary_key()
                            .not_null()
                            .integer()
                            .auto_increment(),
                    )
                    .col(ColumnDef::new(LogTopicInfo::Address).binary_len(20).not_null())
                    .col(ColumnDef::new(LogTopicInfo::Topic).binary_len(32).not_null())
                    .to_owned(),
            )
            .await?;

        // Recreate the index
        manager
            .create_index(
                Index::create()
                    .name("idx_contract_log_topic")
                    .table(LogTopicInfo::Table)
                    .col(LogTopicInfo::Address)
                    .col(LogTopicInfo::Topic)
                    .unique()
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Recreate table with string column
        let _ = manager
            .drop_index(
                Index::drop()
                    .name("idx_contract_log_topic")
                    .table(LogTopicInfo::Table)
                    .to_owned(),
            )
            .await;

        manager
            .drop_table(Table::drop().table(LogTopicInfo::Table).to_owned())
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(LogTopicInfo::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(LogTopicInfo::Id)
                            .primary_key()
                            .not_null()
                            .integer()
                            .auto_increment(),
                    )
                    .col(ColumnDef::new(LogTopicInfo::Address).binary_len(20).not_null())
                    .col(ColumnDef::new(LogTopicInfo::Topic).string_len(64).not_null())
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_contract_log_topic")
                    .table(LogTopicInfo::Table)
                    .col(LogTopicInfo::Address)
                    .col(LogTopicInfo::Topic)
                    .unique()
                    .to_owned(),
            )
            .await
    }
}

#[derive(DeriveIden)]
enum LogTopicInfo {
    Table,
    Id,
    Address,
    Topic,
}
