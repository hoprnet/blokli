use sea_orm_migration::prelude::*;

const INDEX_NAME: &str = "idx_safe_contract_state_chain_key";

/// Indexes Safe contract states by the chain key of their node.
///
/// `safeByChainKey` and `safeBy(CHAIN_KEY)` filter the `safe_contract_current` view by chain key.
/// Without an index the database scans every Safe contract state for each lookup.
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
                    .table(HoprSafeContractState::Table)
                    .col(HoprSafeContractState::ChainKey)
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
                    .table(HoprSafeContractState::Table)
                    .to_owned(),
            )
            .await
    }
}

#[derive(DeriveIden)]
enum HoprSafeContractState {
    Table,
    ChainKey,
}
