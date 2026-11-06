use sea_orm_migration::prelude::*;

const INDEX_NAME: &str = "idx_account_state_safe_address";

/// Indexes account states by their associated Safe address.
///
/// Channel queries resolve the accounts currently linked to a Safe through the
/// `account_current` view. Without this index, that lookup scans account-state
/// history before the view retains only the latest state for each account.
#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name(INDEX_NAME)
                    .table(AccountState::Table)
                    .col(AccountState::SafeAddress)
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_index(
                Index::drop()
                    .if_exists()
                    .name(INDEX_NAME)
                    .table(AccountState::Table)
                    .to_owned(),
            )
            .await
    }
}

#[derive(DeriveIden)]
enum AccountState {
    Table,
    SafeAddress,
}
