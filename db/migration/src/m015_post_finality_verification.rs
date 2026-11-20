use sea_orm_migration::prelude::*;

/// Adds the post-finality verification halt marker column to `chain_info`.
///
/// - `verification_failed_at`: first block of a corroborated range that does not match. The value is NULL when no
///   unresolved mismatch exists.
#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
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
            .await
    }
}

#[derive(DeriveIden)]
enum ChainInfo {
    Table,
    VerificationFailedAt,
}
