use sea_orm_migration::prelude::*;

const INDEX_NAME: &str = "idx_safe_contract_state_module_address";

/// Indexes Safe contract states by their HOPR node management module address.
///
/// Confirmed transactions whose target is a module resolve the owning Safe by filtering the
/// `safe_contract_current` view by module address. Without an index the database scans every
/// Safe contract state for each lookup.
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
                    .col(HoprSafeContractState::ModuleAddress)
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
    ModuleAddress,
}
