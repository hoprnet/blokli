use sea_orm_migration::prelude::*;

const UNIQUE_INDEX_NAME: &str = "idx_account_unique_chain_key";
const PLAIN_INDEX_NAME: &str = "idx_account_chain_key";

/// Makes the chain key the unique identity of an account.
///
/// Only `(chain_key, packet_key)` used to be unique, so a node binding a second packet key got a
/// second account row. Duplicates other than the earliest binding of each chain key are removed
/// before the unique index is created. The schema version bump that ships with this migration
/// clears the data for a re-sync, so nothing derived from the removed rows is kept.
#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared(
                "DELETE FROM account WHERE EXISTS (SELECT 1 FROM account AS earlier WHERE earlier.chain_key = \
                 account.chain_key AND (earlier.published_block, earlier.published_tx_index, \
                 earlier.published_log_index, earlier.id) < (account.published_block, account.published_tx_index, \
                 account.published_log_index, account.id))",
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name(UNIQUE_INDEX_NAME)
                    .table(Account::Table)
                    .col(Account::ChainKey)
                    .unique()
                    .to_owned(),
            )
            .await?;

        // Superseded by the unique index.
        manager
            .drop_index(
                Index::drop()
                    .if_exists()
                    .name(PLAIN_INDEX_NAME)
                    .table(Account::Table)
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name(PLAIN_INDEX_NAME)
                    .table(Account::Table)
                    .col(Account::ChainKey)
                    .to_owned(),
            )
            .await?;

        manager
            .drop_index(
                Index::drop()
                    .if_exists()
                    .name(UNIQUE_INDEX_NAME)
                    .table(Account::Table)
                    .to_owned(),
            )
            .await
    }
}

#[derive(DeriveIden)]
enum Account {
    Table,
    ChainKey,
}
