use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_index(
                Index::drop()
                    .table(HoprSafeContract::Table)
                    .name("idx_hopr_safe_contract_chain_key")
                    .if_exists()
                    .to_owned(),
            )
            .await?;
        manager
            .drop_index(
                Index::drop()
                    .table(HoprSafeContract::Table)
                    .name("idx_hopr_safe_contract_address")
                    .if_exists()
                    .to_owned(),
            )
            .await?;
        manager
            .drop_index(
                Index::drop()
                    .table(HoprSafeContract::Table)
                    .name("idx_hopr_safe_contract_event")
                    .if_exists()
                    .to_owned(),
            )
            .await?;
        manager
            .drop_table(Table::drop().table(HoprSafeContract::Table).if_exists().to_owned())
            .await?;

        // Create HoprSafeContract table with new schema
        manager
            .create_table(
                Table::create()
                    .table(HoprSafeContract::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(HoprSafeContract::Id)
                            .big_integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(HoprSafeContract::Address).binary_len(20).not_null())
                    .col(
                        ColumnDef::new(HoprSafeContract::ModuleAddress)
                            .binary_len(20)
                            .not_null(),
                    )
                    .col(ColumnDef::new(HoprSafeContract::ChainKey).binary_len(20).not_null())
                    .col(ColumnDef::new(HoprSafeContract::DeployedBlock).big_integer().not_null())
                    .col(
                        ColumnDef::new(HoprSafeContract::DeployedTxIndex)
                            .big_integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(HoprSafeContract::DeployedLogIndex)
                            .big_integer()
                            .not_null(),
                    )
                    .to_owned(),
            )
            .await?;

        // Add indices for lookups
        manager
            .create_index(
                Index::create()
                    .table(HoprSafeContract::Table)
                    .name("idx_hopr_safe_contract_chain_key")
                    .col(HoprSafeContract::ChainKey)
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .table(HoprSafeContract::Table)
                    .name("idx_hopr_safe_contract_module_address")
                    .col(HoprSafeContract::ModuleAddress)
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .table(HoprSafeContract::Table)
                    .name("idx_hopr_safe_contract_address")
                    .col(HoprSafeContract::Address)
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .table(HoprSafeContract::Table)
                    .name("idx_hopr_safe_contract_address_module_address")
                    .col(HoprSafeContract::Address)
                    .col(HoprSafeContract::ModuleAddress)
                    .unique()
                    .to_owned(),
            )
            .await?;

        // Add idempotency constraint on event info
        manager
            .create_index(
                Index::create()
                    .table(HoprSafeContract::Table)
                    .name("idx_hopr_safe_contract_event")
                    .col(HoprSafeContract::DeployedBlock)
                    .col(HoprSafeContract::DeployedTxIndex)
                    .col(HoprSafeContract::DeployedLogIndex)
                    .unique()
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_index(
                Index::drop()
                    .table(HoprSafeContract::Table)
                    .name("idx_hopr_safe_contract_chain_key")
                    .if_exists()
                    .to_owned(),
            )
            .await?;
        manager
            .drop_index(
                Index::drop()
                    .table(HoprSafeContract::Table)
                    .name("idx_hopr_safe_contract_module_address")
                    .if_exists()
                    .to_owned(),
            )
            .await?;
        manager
            .drop_index(
                Index::drop()
                    .table(HoprSafeContract::Table)
                    .name("idx_hopr_safe_contract_address")
                    .if_exists()
                    .to_owned(),
            )
            .await?;
        manager
            .drop_index(
                Index::drop()
                    .table(HoprSafeContract::Table)
                    .name("idx_hopr_safe_contract_address_module_address")
                    .if_exists()
                    .to_owned(),
            )
            .await?;
        manager
            .drop_index(
                Index::drop()
                    .table(HoprSafeContract::Table)
                    .name("idx_hopr_safe_contract_event")
                    .if_exists()
                    .to_owned(),
            )
            .await?;

        // Drop the new table
        manager
            .drop_table(Table::drop().table(HoprSafeContract::Table).to_owned())
            .await?;

        // Create HoprSafeContract table with new schema
        manager
            .create_table(
                Table::create()
                    .table(HoprSafeContract::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(HoprSafeContract::Id)
                            .big_integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(HoprSafeContract::Address)
                            .binary_len(20)
                            .not_null()
                            .unique_key(),
                    )
                    .col(
                        ColumnDef::new(HoprSafeContract::ModuleAddress)
                            .binary_len(20)
                            .not_null(),
                    )
                    .col(ColumnDef::new(HoprSafeContract::ChainKey).binary_len(20).not_null())
                    .col(ColumnDef::new(HoprSafeContract::DeployedBlock).big_integer().not_null())
                    .col(
                        ColumnDef::new(HoprSafeContract::DeployedTxIndex)
                            .big_integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(HoprSafeContract::DeployedLogIndex)
                            .big_integer()
                            .not_null(),
                    )
                    .to_owned(),
            )
            .await?;

        // Add indices for lookups
        manager
            .create_index(
                Index::create()
                    .table(HoprSafeContract::Table)
                    .name("idx_hopr_safe_contract_chain_key")
                    .col(HoprSafeContract::ChainKey)
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .table(HoprSafeContract::Table)
                    .name("idx_hopr_safe_contract_address")
                    .col(HoprSafeContract::Address)
                    .to_owned(),
            )
            .await?;

        // Add idempotency constraint on event info
        manager
            .create_index(
                Index::create()
                    .table(HoprSafeContract::Table)
                    .name("idx_hopr_safe_contract_event")
                    .col(HoprSafeContract::DeployedBlock)
                    .col(HoprSafeContract::DeployedTxIndex)
                    .col(HoprSafeContract::DeployedLogIndex)
                    .unique()
                    .to_owned(),
            )
            .await?;

        Ok(())
    }
}

#[derive(DeriveIden)]
enum HoprSafeContract {
    Table,
    Id,
    Address,
    ModuleAddress,
    ChainKey,
    DeployedBlock,
    DeployedTxIndex,
    DeployedLogIndex,
}
