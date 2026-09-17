use sea_orm_migration::prelude::*;

/// Persists progress for the two-pass historical synchronisation session.
#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(HistoricalSyncProgress::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(HistoricalSyncProgress::Id)
                            .big_integer()
                            .not_null()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(HistoricalSyncProgress::RangeStart)
                            .big_integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(HistoricalSyncProgress::RangeEnd)
                            .big_integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(HistoricalSyncProgress::DiscoveryNext)
                            .big_integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(HistoricalSyncProgress::BackfillNext)
                            .big_integer()
                            .not_null(),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(
                Table::drop()
                    .table(HistoricalSyncProgress::Table)
                    .if_exists()
                    .to_owned(),
            )
            .await
    }
}

#[derive(DeriveIden)]
enum HistoricalSyncProgress {
    Table,
    Id,
    RangeStart,
    RangeEnd,
    DiscoveryNext,
    BackfillNext,
}
