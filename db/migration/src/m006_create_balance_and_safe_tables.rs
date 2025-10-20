use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Create HoprBalance table
        manager
            .create_table(
                Table::create()
                    .table(HoprBalance::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(HoprBalance::Id)
                            .integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(HoprBalance::Address)
                            .string_len(40)
                            .not_null()
                            .unique_key(),
                    )
                    .col(
                        ColumnDef::new(HoprBalance::Balance)
                            .binary_len(12)
                            .not_null()
                            .default(vec![0u8; 12]),
                    )
                    .col(
                        ColumnDef::new(HoprBalance::LastChangedBlock)
                            .binary_len(8)
                            .not_null()
                            .default(vec![0u8; 8]),
                    )
                    .col(
                        ColumnDef::new(HoprBalance::LastChangedTxIndex)
                            .binary_len(8)
                            .not_null()
                            .default(vec![0u8; 8]),
                    )
                    .col(
                        ColumnDef::new(HoprBalance::LastChangedLogIndex)
                            .binary_len(8)
                            .not_null()
                            .default(vec![0u8; 8]),
                    )
                    .to_owned(),
            )
            .await?;

        // Create NativeBalance table
        manager
            .create_table(
                Table::create()
                    .table(NativeBalance::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(NativeBalance::Id)
                            .integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(NativeBalance::Address)
                            .string_len(40)
                            .not_null()
                            .unique_key(),
                    )
                    .col(
                        ColumnDef::new(NativeBalance::Balance)
                            .binary_len(12)
                            .not_null()
                            .default(vec![0u8; 12]),
                    )
                    .col(
                        ColumnDef::new(NativeBalance::LastChangedBlock)
                            .binary_len(8)
                            .not_null()
                            .default(vec![0u8; 8]),
                    )
                    .col(
                        ColumnDef::new(NativeBalance::LastChangedTxIndex)
                            .binary_len(8)
                            .not_null()
                            .default(vec![0u8; 8]),
                    )
                    .col(
                        ColumnDef::new(NativeBalance::LastChangedLogIndex)
                            .binary_len(8)
                            .not_null()
                            .default(vec![0u8; 8]),
                    )
                    .to_owned(),
            )
            .await?;

        // Create HoprSafeContract table
        manager
            .create_table(
                Table::create()
                    .table(HoprSafeContract::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(HoprSafeContract::Id)
                            .integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(HoprSafeContract::Address)
                            .string_len(40)
                            .not_null()
                            .unique_key(),
                    )
                    .col(ColumnDef::new(HoprSafeContract::DeployedBlock).binary_len(8).not_null())
                    .col(
                        ColumnDef::new(HoprSafeContract::DeployedTxIndex)
                            .binary_len(8)
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(HoprSafeContract::DeployedLogIndex)
                            .binary_len(8)
                            .not_null(),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(HoprSafeContract::Table).to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(NativeBalance::Table).to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(HoprBalance::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum HoprBalance {
    Table,
    Id,
    Address,
    Balance,
    LastChangedBlock,
    LastChangedTxIndex,
    LastChangedLogIndex,
}

#[derive(DeriveIden)]
enum NativeBalance {
    Table,
    Id,
    Address,
    Balance,
    LastChangedBlock,
    LastChangedTxIndex,
    LastChangedLogIndex,
}

#[derive(DeriveIden)]
enum HoprSafeContract {
    Table,
    Id,
    Address,
    DeployedBlock,
    DeployedTxIndex,
    DeployedLogIndex,
}
