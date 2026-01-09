use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    /// Applies the migration that replaces the existing `hopr_safe_contract` table with a new schema.
    ///
    /// The migration drops any existing `hopr_safe_contract` table, creates a new table that includes
    /// `module_address` and `chain_key` columns, and adds indices for `chain_key`, `address`, and a
    /// unique composite index on `(deployed_block, deployed_tx_index, deployed_log_index)` to enforce
    /// event idempotency.
    ///
    /// # Parameters
    ///
    /// - `manager`: schema manager used to execute the migration operations.
    ///
    /// # Returns
    ///
    /// `Ok(())` on success, `Err(DbErr)` if any schema operation fails.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// # use sea_orm_migration::prelude::*;
    /// # use sea_query::SchemaManager;
    /// # use sea_orm::DbErr;
    /// # struct Migration;
    /// # impl Migration {
    /// #     async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> { Ok(()) }
    /// # }
    /// # async fn run_example() -> Result<(), DbErr> {
    /// let manager: SchemaManager = /* obtain SchemaManager from migration context */ unimplemented!();
    /// let migration = Migration;
    /// migration.up(&manager).await?;
    /// # Ok(())
    /// # }
    /// ```
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Drop existing table to enforce new schema with module_address and chain_key columns.
        // NOTE: This migration requires full re-indexing of Safe deployment events from the blockchain.
        // This is acceptable because Safe deployments are infrequent (once per HOPR node) and re-indexing
        // is fast. The StakeFactory.NewHoprNodeStakeModuleForSafe events will repopulate this table.
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
                    .name("idx_uq_hopr_safe_contract_address_module_address")
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
                    .name("idx_uq_hopr_safe_contract_event")
                    .col(HoprSafeContract::DeployedBlock)
                    .col(HoprSafeContract::DeployedTxIndex)
                    .col(HoprSafeContract::DeployedLogIndex)
                    .unique()
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    /// Reverts the migration by replacing the updated `HoprSafeContract` table with its previous schema.
    ///
    /// This drops the table created by the migration and recreates the old `HoprSafeContract` table
    /// without `ModuleAddress` and `ChainKey`, restoring columns: `Id`, `Address`, `DeployedBlock`,
    /// `DeployedTxIndex`, and `DeployedLogIndex`.
    ///
    /// # Parameters
    ///
    /// - `manager`: Schema manager used to execute the drop and create table operations.
    ///
    /// # Returns
    ///
    /// `Ok(())` on success, `DbErr` on failure.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// # use sea_orm_migration::prelude::*;
    /// # async fn example(manager: &SchemaManager) -> Result<(), DbErr> {
    /// let migration = Migration;
    /// migration.down(manager).await?;
    /// # Ok(())
    /// # }
    /// ```
    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Drop the new table
        manager
            .drop_table(Table::drop().table(HoprSafeContract::Table).to_owned())
            .await?;

        // Recreate the old table (without new columns)
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
