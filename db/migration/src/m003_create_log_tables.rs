use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Log and LogStatus tables are kept separate to allow for easier export of the logs
        // themselves.

        manager
            .create_table(
                Table::create()
                    .table(Log::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(Log::Id)
                            .integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(Log::LogStatusId).integer().null())
                    .col(ColumnDef::new(Log::TxIndex).not_null().binary_len(8))
                    .col(ColumnDef::new(Log::LogIndex).not_null().binary_len(8))
                    .col(ColumnDef::new(Log::BlockNumber).not_null().binary_len(8))
                    .col(ColumnDef::new(Log::BlockHash).binary_len(32).not_null())
                    .col(ColumnDef::new(Log::TransactionHash).binary_len(32).not_null())
                    .col(ColumnDef::new(Log::Address).binary_len(20).not_null())
                    .col(ColumnDef::new(Log::Topics).binary().not_null())
                    .col(ColumnDef::new(Log::Data).binary().not_null())
                    .col(ColumnDef::new(Log::Removed).boolean().not_null().default(false))
                    .to_owned(),
            )
            .await?;

        // Add unique constraint on composite key (tx_index, log_index, block_number)
        manager
            .create_index(
                Index::create()
                    .name("idx_log_composite")
                    .table(Log::Table)
                    .col(Log::BlockNumber)
                    .col(Log::TxIndex)
                    .col(Log::LogIndex)
                    .unique()
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(LogStatus::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(LogStatus::Id)
                            .integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(LogStatus::LogId).integer().not_null())
                    .col(ColumnDef::new(LogStatus::TxIndex).not_null().binary_len(8))
                    .col(ColumnDef::new(LogStatus::LogIndex).not_null().binary_len(8))
                    .col(ColumnDef::new(LogStatus::BlockNumber).not_null().binary_len(8))
                    .col(ColumnDef::new(LogStatus::Processed).boolean().not_null().default(false))
                    .col(ColumnDef::new(LogStatus::ProcessedAt).date_time())
                    .col(ColumnDef::new(LogStatus::Checksum).binary_len(32))
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_log_status_log_id")
                            .from(LogStatus::Table, LogStatus::LogId)
                            .to(Log::Table, Log::Id)
                            .on_delete(ForeignKeyAction::Cascade)
                            .on_update(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        // Add unique constraint on composite key (tx_index, log_index, block_number)
        manager
            .create_index(
                Index::create()
                    .name("idx_log_status_composite")
                    .table(LogStatus::Table)
                    .col(LogStatus::BlockNumber)
                    .col(LogStatus::TxIndex)
                    .col(LogStatus::LogIndex)
                    .unique()
                    .to_owned(),
            )
            .await?;

        // Create LogTopicInfo table
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
                    .col(ColumnDef::new(LogTopicInfo::Address).string_len(40).not_null())
                    .col(ColumnDef::new(LogTopicInfo::Topic).string_len(64).not_null())
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(LogTopicInfo::Table).to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(LogStatus::Table).to_owned())
            .await?;
        manager.drop_table(Table::drop().table(Log::Table).to_owned()).await
    }
}

#[allow(clippy::enum_variant_names)]
#[derive(DeriveIden)]
enum Log {
    Table,
    Id,
    LogStatusId,
    // address from which this log originated.
    Address,
    // Array of 0 to 4 32 Bytes DATA of indexed log arguments. The first topic is the
    // hash of the signature of the event.
    Topics,
    // contains zero or more 32 Bytes non-indexed arguments of the log.
    Data,
    // the block number where this log was in. null when it's a pending log.
    BlockNumber,
    // hash of the transactions this log was created from. null when its pending log.
    // hash of the transaction this log was created from. null when it's a pending log.
    TransactionHash,
    // integer of the transaction's index position this log was created from. null when it's a pending log.
    TxIndex,
    // hash of the block where this log was in. null when its pending. null when its pending log.
    BlockHash,
    // integer of the log index position in the block. null when its pending log.
    LogIndex,
    // true when the log was removed, due to a chain reorganization. false if its a valid log.
    Removed,
}

#[derive(DeriveIden)]
enum LogStatus {
    Table,
    Id,
    LogId,
    // Values to identify the log.
    BlockNumber,
    TxIndex,
    LogIndex,
    // Indicates whether the log has been processed.
    Processed,
    // Time when the log was processed.
    ProcessedAt,
    // Computed checksum of this log and previous logs
    Checksum,
}

#[derive(DeriveIden)]
enum LogTopicInfo {
    Table,
    Id,
    /// Contract address for filter
    Address,
    /// Topic for the contract on this address
    Topic,
}
