use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Add new columns - Split for SQLite compatibility
        manager
            .alter_table(
                Table::alter()
                    .table(HoprSafeContract::Table)
                    .add_column(
                        ColumnDef::new(HoprSafeContract::ModuleAddress)
                            .binary_len(20)
                            .not_null()
                            .default(vec![0u8; 20]), // Add default to handle existing rows
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .alter_table(
                Table::alter()
                    .table(HoprSafeContract::Table)
                    .add_column(
                        ColumnDef::new(HoprSafeContract::ChainKey)
                            .binary_len(20)
                            .not_null()
                            .default(vec![0u8; 20]), // Add default to handle existing rows
                    )
                    .to_owned(),
            )
            .await?;

        // We can remove the defaults now that columns are added
        // But SeaORM doesn't easily support dropping defaults in the same statement or easily cross-db.
        // Since this is an append-only table for us mostly, and existing data might be empty or we accept default 0s
        // for old entries (which likely don't exist or are few). The design implies we start using this new
        // structure.

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

        // Address already has a unique constraint, but an explicit index might be desired for performance if the unique
        // constraint implementation doesn't provide optimal lookup or if we want a non-unique index name (though it is
        // unique). However, if we create a standard index on a unique column it works.
        manager
            .create_index(
                Index::create()
                    .table(HoprSafeContract::Table)
                    .name("idx_hopr_safe_contract_address")
                    .col(HoprSafeContract::Address)
                    .to_owned(),
            )
            .await?;

        // Add idempotency constraint on event provenance
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
        // Drop indices
        manager
            .drop_index(
                Index::drop()
                    .name("idx_hopr_safe_contract_event")
                    .table(HoprSafeContract::Table)
                    .to_owned(),
            )
            .await?;

        manager
            .drop_index(
                Index::drop()
                    .name("idx_hopr_safe_contract_address")
                    .table(HoprSafeContract::Table)
                    .to_owned(),
            )
            .await?;

        manager
            .drop_index(
                Index::drop()
                    .name("idx_hopr_safe_contract_chain_key")
                    .table(HoprSafeContract::Table)
                    .to_owned(),
            )
            .await?;

        // Drop columns
        manager
            .alter_table(
                Table::alter()
                    .table(HoprSafeContract::Table)
                    .drop_column(HoprSafeContract::ChainKey)
                    .drop_column(HoprSafeContract::ModuleAddress)
                    .to_owned(),
            )
            .await?;

        Ok(())
    }
}

#[derive(DeriveIden)]
enum HoprSafeContract {
    Table,
    Address,
    ModuleAddress,
    ChainKey,
    DeployedBlock,
    DeployedTxIndex,
    DeployedLogIndex,
}
