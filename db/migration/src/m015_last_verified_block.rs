use sea_orm_migration::prelude::*;

/// Adds the last block that post-finality verification checked to `chain_info`.
///
/// The verification position has to outlive the process. Blocks indexed above finality before a
/// restart can become final while the daemon is down; with an in-memory position a restarted
/// process starts at the finalized block of that moment and never checks them.
#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(ChainInfo::Table)
                    .add_column(ColumnDef::new(ChainInfo::LastVerifiedBlock).big_integer().null())
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(ChainInfo::Table)
                    .drop_column(ChainInfo::LastVerifiedBlock)
                    .to_owned(),
            )
            .await
    }
}

#[derive(DeriveIden)]
enum ChainInfo {
    Table,
    LastVerifiedBlock,
}
