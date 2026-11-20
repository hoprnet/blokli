use sea_orm_migration::prelude::*;

/// Adds post-finality verification state columns to `chain_info`:
/// - `last_verified_block`: last block whose stored logs matched canonical finalized logs
/// - `verification_failed_at`: first block of a corroborated mismatching range (NULL when clear)
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
            .await?;

        manager
            .alter_table(
                Table::alter()
                    .table(ChainInfo::Table)
                    .add_column(ColumnDef::new(ChainInfo::VerificationFailedAt).big_integer().null())
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(ChainInfo::Table)
                    .drop_column(ChainInfo::VerificationFailedAt)
                    .to_owned(),
            )
            .await?;

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
    VerificationFailedAt,
}
