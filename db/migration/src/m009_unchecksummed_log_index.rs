use sea_orm_migration::prelude::*;

const INDEX_NAME: &str = "idx_log_status_unchecksummed";

/// Adds a partial index over the logs that have no checksum yet.
///
/// After every block the indexer looks up the logs whose checksum is still `NULL`, in log order.
/// Without this index, PostgreSQL and SQLite find them by scanning all of `log_status` (or by
/// walking `idx_log_status_composite` past every log already checksummed), so the lookup grows
/// with the whole log history even when the block added nothing. The partial index holds only
/// the few unchecksummed rows, which keeps the lookup constant.
#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name(INDEX_NAME)
                    .table(LogStatus::Table)
                    .col(LogStatus::BlockNumber)
                    .col(LogStatus::TxIndex)
                    .col(LogStatus::LogIndex)
                    .and_where(Expr::col(LogStatus::Checksum).is_null())
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_index(
                Index::drop()
                    .if_exists()
                    .name(INDEX_NAME)
                    .table(LogStatus::Table)
                    .to_owned(),
            )
            .await
    }
}

#[derive(DeriveIden)]
enum LogStatus {
    Table,
    BlockNumber,
    TxIndex,
    LogIndex,
    Checksum,
}
