use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(HoprSafeRedeemedStats::Table)
                    .add_column(
                        ColumnDef::new(HoprSafeRedeemedStats::RejectedAmount)
                            .binary_len(32)
                            .not_null()
                            .default(vec![0u8; 32])
                            .to_owned(),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .alter_table(
                Table::alter()
                    .table(HoprSafeRedeemedStats::Table)
                    .add_column(
                        ColumnDef::new(HoprSafeRedeemedStats::RejectionCount)
                            .big_integer()
                            .not_null()
                            .default(0)
                            .to_owned(),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(HoprSafeRedeemedStats::Table)
                    .drop_column(HoprSafeRedeemedStats::RejectedAmount)
                    .to_owned(),
            )
            .await?;

        manager
            .alter_table(
                Table::alter()
                    .table(HoprSafeRedeemedStats::Table)
                    .drop_column(HoprSafeRedeemedStats::RejectionCount)
                    .to_owned(),
            )
            .await
    }
}

#[derive(DeriveIden)]
enum HoprSafeRedeemedStats {
    Table,
    RejectedAmount,
    RejectionCount,
}
