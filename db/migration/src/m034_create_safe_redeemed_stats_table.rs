use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(HoprSafeRedeemedStats::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(HoprSafeRedeemedStats::Id)
                            .big_integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(HoprSafeRedeemedStats::SafeAddress)
                            .binary_len(20)
                            .not_null()
                            .unique_key(),
                    )
                    .col(
                        ColumnDef::new(HoprSafeRedeemedStats::RedeemedAmount)
                            .binary_len(32)
                            .not_null()
                            .default(vec![0u8; 32]),
                    )
                    .col(
                        ColumnDef::new(HoprSafeRedeemedStats::RedemptionCount)
                            .big_integer()
                            .not_null()
                            .default(0),
                    )
                    .col(
                        ColumnDef::new(HoprSafeRedeemedStats::LastRedeemedBlock)
                            .big_integer()
                            .not_null()
                            .default(0),
                    )
                    .col(
                        ColumnDef::new(HoprSafeRedeemedStats::LastRedeemedTxIndex)
                            .big_integer()
                            .not_null()
                            .default(0),
                    )
                    .col(
                        ColumnDef::new(HoprSafeRedeemedStats::LastRedeemedLogIndex)
                            .big_integer()
                            .not_null()
                            .default(0),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(HoprSafeRedeemedStats::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum HoprSafeRedeemedStats {
    Table,
    Id,
    SafeAddress,
    RedeemedAmount,
    RedemptionCount,
    LastRedeemedBlock,
    LastRedeemedTxIndex,
    LastRedeemedLogIndex,
}
