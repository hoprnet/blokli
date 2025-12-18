use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    /// Creates the `hopr_node_safe_registration` table to track node-to-safe bindings.
    ///
    /// This migration adds a new table to store the many-to-one relationship between nodes
    /// and safes: multiple nodes can register to one safe, but each node can only register
    /// to a single safe at a time.
    ///
    /// The table structure enforces:
    /// - One safe can have multiple registered nodes (no unique constraint on safe_address alone)
    /// - One node can only be registered to one safe (unique constraint on node_address)
    /// - No duplicate registrations (unique constraint on (safe_address, node_address))
    /// - Event idempotency (unique constraint on event coordinates)
    ///
    /// # Parameters
    ///
    /// - `manager`: Schema manager used to execute the migration operations.
    ///
    /// # Returns
    ///
    /// `Ok(())` on success, `Err(DbErr)` if any schema operation fails.
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Create hopr_node_safe_registration table
        manager
            .create_table(
                Table::create()
                    .table(HoprNodeSafeRegistration::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(HoprNodeSafeRegistration::Id)
                            .big_integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(HoprNodeSafeRegistration::SafeAddress)
                            .binary_len(20)
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(HoprNodeSafeRegistration::NodeAddress)
                            .binary_len(20)
                            .not_null()
                            .unique_key(),
                    )
                    .col(
                        ColumnDef::new(HoprNodeSafeRegistration::RegisteredBlock)
                            .big_integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(HoprNodeSafeRegistration::RegisteredTxIndex)
                            .big_integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(HoprNodeSafeRegistration::RegisteredLogIndex)
                            .big_integer()
                            .not_null(),
                    )
                    .to_owned(),
            )
            .await?;

        // Add composite unique constraint on (safe_address, node_address)
        // Prevents duplicate registrations of the same node to the same safe
        manager
            .create_index(
                Index::create()
                    .table(HoprNodeSafeRegistration::Table)
                    .name("idx_hopr_node_safe_registration_binding")
                    .col(HoprNodeSafeRegistration::SafeAddress)
                    .col(HoprNodeSafeRegistration::NodeAddress)
                    .unique()
                    .to_owned(),
            )
            .await?;

        // Add index on safe_address for efficient node lookups
        manager
            .create_index(
                Index::create()
                    .table(HoprNodeSafeRegistration::Table)
                    .name("idx_hopr_node_safe_registration_safe")
                    .col(HoprNodeSafeRegistration::SafeAddress)
                    .to_owned(),
            )
            .await?;

        // Add unique constraint on event coordinates for idempotency
        // Prevents processing the same RegisteredNodeSafe event multiple times
        manager
            .create_index(
                Index::create()
                    .table(HoprNodeSafeRegistration::Table)
                    .name("idx_hopr_node_safe_registration_event")
                    .col(HoprNodeSafeRegistration::RegisteredBlock)
                    .col(HoprNodeSafeRegistration::RegisteredTxIndex)
                    .col(HoprNodeSafeRegistration::RegisteredLogIndex)
                    .unique()
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    /// Reverts the migration by dropping the `hopr_node_safe_registration` table.
    ///
    /// This removes the node-safe registration tracking table and all its indices.
    ///
    /// # Parameters
    ///
    /// - `manager`: Schema manager used to execute the drop table operation.
    ///
    /// # Returns
    ///
    /// `Ok(())` on success, `Err(DbErr)` on failure.
    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(
                Table::drop()
                    .table(HoprNodeSafeRegistration::Table)
                    .if_exists()
                    .to_owned(),
            )
            .await
    }
}

#[derive(DeriveIden)]
enum HoprNodeSafeRegistration {
    Table,
    Id,
    SafeAddress,
    NodeAddress,
    RegisteredBlock,
    RegisteredTxIndex,
    RegisteredLogIndex,
}
