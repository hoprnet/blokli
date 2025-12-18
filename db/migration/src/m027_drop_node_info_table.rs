use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Drop the node_info table as Blokli instances are no longer tied to a single node
        // Balance and allowance queries will be handled on-demand via RPC instead

        manager
            .drop_table(Table::drop().table(NodeInfo::Table).if_exists().to_owned())
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Recreate node_info table for rollback
        // This matches the original schema from m001_create_index_tables.rs

        manager
            .create_table(
                Table::create()
                    .table(NodeInfo::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(NodeInfo::Id)
                            .big_integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(NodeInfo::SafeBalance).string().not_null())
                    .col(ColumnDef::new(NodeInfo::SafeAllowance).string().not_null())
                    .col(ColumnDef::new(NodeInfo::SafeAddress).binary().not_null())
                    .col(ColumnDef::new(NodeInfo::ModuleAddress).binary().not_null())
                    .to_owned(),
            )
            .await
    }
}

#[derive(DeriveIden)]
enum NodeInfo {
    Table,
    Id,
    SafeBalance,
    SafeAllowance,
    SafeAddress,
    ModuleAddress,
}
