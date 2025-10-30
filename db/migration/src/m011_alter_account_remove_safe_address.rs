use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Drop safe_address column from account table
        // This field is now stored in account_state for full version history
        manager
            .alter_table(
                Table::alter()
                    .table(Account::Table)
                    .drop_column(Account::SafeAddress)
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Restore safe_address column
        manager
            .alter_table(
                Table::alter()
                    .table(Account::Table)
                    .add_column(ColumnDef::new(Account::SafeAddress).binary_len(20).null())
                    .to_owned(),
            )
            .await
    }
}

#[derive(DeriveIden)]
enum Account {
    Table,
    SafeAddress,
}
