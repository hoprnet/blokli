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
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(HoprSafeRedeemedStats::NodeAddress)
                            .binary_len(20)
                            .not_null(),
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
            .await?;

        manager
            .create_index(
                Index::create()
                    .table(HoprSafeRedeemedStats::Table)
                    .name("idx_hopr_safe_redeemed_stats_safe_node_unique")
                    .col(HoprSafeRedeemedStats::SafeAddress)
                    .col(HoprSafeRedeemedStats::NodeAddress)
                    .unique()
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .table(HoprSafeRedeemedStats::Table)
                    .name("idx_hopr_safe_redeemed_stats_safe")
                    .col(HoprSafeRedeemedStats::SafeAddress)
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .table(HoprSafeRedeemedStats::Table)
                    .name("idx_hopr_safe_redeemed_stats_node")
                    .col(HoprSafeRedeemedStats::NodeAddress)
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_index(
                Index::drop()
                    .table(HoprSafeRedeemedStats::Table)
                    .name("idx_hopr_safe_redeemed_stats_safe_node_unique")
                    .if_exists()
                    .to_owned(),
            )
            .await?;

        manager
            .drop_index(
                Index::drop()
                    .table(HoprSafeRedeemedStats::Table)
                    .name("idx_hopr_safe_redeemed_stats_safe")
                    .if_exists()
                    .to_owned(),
            )
            .await?;

        manager
            .drop_index(
                Index::drop()
                    .table(HoprSafeRedeemedStats::Table)
                    .name("idx_hopr_safe_redeemed_stats_node")
                    .if_exists()
                    .to_owned(),
            )
            .await?;

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
    NodeAddress,
    RedeemedAmount,
    RedemptionCount,
    LastRedeemedBlock,
    LastRedeemedTxIndex,
    LastRedeemedLogIndex,
}
