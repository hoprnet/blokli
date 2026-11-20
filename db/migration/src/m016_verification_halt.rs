use sea_orm_migration::prelude::*;

/// Keep a corroborated verification failure until the index is restored or rebuilt.
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
