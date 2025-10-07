use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // This index enables fast querying for the next unprocessed log
        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx_unprocessed_log_status")
                    .table(LogStatus::Table)
                    .col(LogStatus::Processed)
                    .col(LogStatus::BlockNumber)
                    .col(LogStatus::TransactionIndex)
                    .col(LogStatus::LogIndex)
                    .to_owned(),
            )
            .await?;

        // Index for efficient log status querying by block number and processed status
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

        // Index for contract log topic filtering
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
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_index(Index::drop().name("idx_contract_log_topic").to_owned())
            .await?;
        manager
            .drop_index(Index::drop().name("idx_log_status_block_number_processed").to_owned())
            .await?;
        manager
            .drop_index(Index::drop().name("idx_unprocessed_log_status").to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum LogStatus {
    Table,
    BlockNumber,
    TransactionIndex,
    LogIndex,
    Processed,
}

#[derive(DeriveIden)]
enum LogTopicInfo {
    Table,
    Address,
    Topic,
}
