use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Create AccountState table
        // Stores mutable account fields with full version history
        // Using big integers for position fields to match existing schema pattern
        manager
            .create_table(
                Table::create()
                    .table(AccountState::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(AccountState::Id)
                            .integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(AccountState::AccountId).integer().not_null())
                    .col(ColumnDef::new(AccountState::SafeAddress).binary_len(20).null())
                    .col(ColumnDef::new(AccountState::PublishedBlock).big_integer().not_null())
                    .col(ColumnDef::new(AccountState::PublishedTxIndex).big_integer().not_null())
                    .col(ColumnDef::new(AccountState::PublishedLogIndex).big_integer().not_null())
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_account_state_account_id")
                            .from(AccountState::Table, AccountState::AccountId)
                            .to(Account::Table, Account::Id)
                            .on_delete(ForeignKeyAction::Cascade)
                            .on_update(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        // Create unique constraint on (account_id, published_block, published_tx_index, published_log_index)
        manager
            .create_index(
                Index::create()
                    .name("idx_account_state_unique_position")
                    .table(AccountState::Table)
                    .col(AccountState::AccountId)
                    .col(AccountState::PublishedBlock)
                    .col(AccountState::PublishedTxIndex)
                    .col(AccountState::PublishedLogIndex)
                    .unique()
                    .to_owned(),
            )
            .await?;

        // Create index for efficient "latest state at or before position" queries
        manager
            .create_index(
                Index::create()
                    .name("idx_account_state_position")
                    .table(AccountState::Table)
                    .col(AccountState::AccountId)
                    .col((AccountState::PublishedBlock, IndexOrder::Desc))
                    .col((AccountState::PublishedTxIndex, IndexOrder::Desc))
                    .col((AccountState::PublishedLogIndex, IndexOrder::Desc))
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(AccountState::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum AccountState {
    Table,
    Id,
    AccountId,
    SafeAddress,
    PublishedBlock,
    PublishedTxIndex,
    PublishedLogIndex,
}

#[derive(DeriveIden)]
enum Account {
    Table,
    Id,
}
