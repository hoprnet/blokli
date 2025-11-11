use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Add transaction and log index to watermark for precise block position tracking
        // Extends watermark from (block) to (block, tx_index, log_index)
        manager
            .alter_table(
                Table::alter()
                    .table(ChainInfo::Table)
                    .add_column(
                        ColumnDef::new(ChainInfo::LastIndexedTxIndex)
                            .big_integer()
                            .not_null()
                            .default(0),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .alter_table(
                Table::alter()
                    .table(ChainInfo::Table)
                    .add_column(
                        ColumnDef::new(ChainInfo::LastIndexedLogIndex)
                            .big_integer()
                            .not_null()
                            .default(0),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Remove watermark index fields
        manager
            .alter_table(
                Table::alter()
                    .table(ChainInfo::Table)
                    .drop_column(ChainInfo::LastIndexedTxIndex)
                    .to_owned(),
            )
            .await?;

        manager
            .alter_table(
                Table::alter()
                    .table(ChainInfo::Table)
                    .drop_column(ChainInfo::LastIndexedLogIndex)
                    .to_owned(),
            )
            .await
    }
}

#[derive(DeriveIden)]
enum ChainInfo {
    Table,
    LastIndexedTxIndex,
    LastIndexedLogIndex,
}
