use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // === log ===
        manager
            .create_table(
                Table::create()
                    .table(Log::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(Log::Id)
                            .big_integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(Log::TxIndex).big_integer().not_null())
                    .col(ColumnDef::new(Log::LogIndex).big_integer().not_null())
                    .col(ColumnDef::new(Log::BlockNumber).big_integer().not_null())
                    .col(ColumnDef::new(Log::BlockHash).binary_len(32).not_null())
                    .col(ColumnDef::new(Log::TransactionHash).binary_len(32).not_null())
                    .col(ColumnDef::new(Log::Address).binary_len(20).not_null())
                    .col(ColumnDef::new(Log::Topics).binary().not_null())
                    .col(ColumnDef::new(Log::Data).binary().not_null())
                    .col(ColumnDef::new(Log::Removed).boolean().not_null().default(false))
                    .to_owned(),
            )
            .await?;

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

        // === log_status ===
        manager
            .create_table(
                Table::create()
                    .table(LogStatus::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(LogStatus::Id)
                            .big_integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(LogStatus::LogId).big_integer().not_null())
                    .col(ColumnDef::new(LogStatus::TxIndex).big_integer().not_null())
                    .col(ColumnDef::new(LogStatus::LogIndex).big_integer().not_null())
                    .col(ColumnDef::new(LogStatus::BlockNumber).big_integer().not_null())
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

        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx_unprocessed_log_status")
                    .table(LogStatus::Table)
                    .col(LogStatus::Processed)
                    .col(LogStatus::BlockNumber)
                    .col(LogStatus::TxIndex)
                    .col(LogStatus::LogIndex)
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx_log_status_block_number_processed")
                    .table(LogStatus::Table)
                    .col(LogStatus::BlockNumber)
                    .col(LogStatus::Processed)
                    .to_owned(),
            )
            .await?;

        // === log_topic_info (binary topic directly) ===
        manager
            .create_table(
                Table::create()
                    .table(LogTopicInfo::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(LogTopicInfo::Id)
                            .big_integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(LogTopicInfo::Address).binary_len(20).not_null())
                    .col(ColumnDef::new(LogTopicInfo::Topic).binary_len(32).not_null())
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx_contract_log_topic")
                    .table(LogTopicInfo::Table)
                    .col(LogTopicInfo::Address)
                    .col(LogTopicInfo::Topic)
                    .unique()
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(LogTopicInfo::Table).if_exists().to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(LogStatus::Table).if_exists().to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(Log::Table).if_exists().to_owned())
            .await?;

        Ok(())
    }
}

#[allow(clippy::enum_variant_names)]
#[derive(DeriveIden)]
enum Log {
    Table,
    Id,
    Address,
    Topics,
    Data,
    BlockNumber,
    TransactionHash,
    TxIndex,
    BlockHash,
    LogIndex,
    Removed,
}

#[derive(DeriveIden)]
enum LogStatus {
    Table,
    Id,
    LogId,
    BlockNumber,
    TxIndex,
    LogIndex,
    Processed,
    ProcessedAt,
    Checksum,
}

#[derive(DeriveIden)]
enum LogTopicInfo {
    Table,
    Id,
    Address,
    Topic,
}
