use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Add channel closure grace period to ChainInfo table
        manager
            .alter_table(
                Table::alter()
                    .table(ChainInfo::Table)
                    .add_column(
                        ColumnDef::new(ChainInfo::ChannelClosureGracePeriod)
                            .big_integer()
                            .null(), // Nullable initially, will be populated by indexer
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Remove channel closure grace period from ChainInfo table
        manager
            .alter_table(
                Table::alter()
                    .table(ChainInfo::Table)
                    .drop_column(ChainInfo::ChannelClosureGracePeriod)
                    .to_owned(),
            )
            .await
    }
}

#[derive(DeriveIden)]
enum ChainInfo {
    Table,
    ChannelClosureGracePeriod,
}