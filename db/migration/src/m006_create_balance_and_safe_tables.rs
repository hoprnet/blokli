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
                            .binary_len(20)
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
                            .big_integer()
                            .not_null()
                            .default(0),
                    )
                    .col(
                        ColumnDef::new(HoprBalance::LastChangedTxIndex)
                            .big_integer()
                            .not_null()
                            .default(0),
                    )
                    .col(
                        ColumnDef::new(HoprBalance::LastChangedLogIndex)
                            .big_integer()
                            .not_null()
                            .default(0),
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
                            .binary_len(20)
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
                            .big_integer()
                            .not_null()
                            .default(0),
                    )
                    .col(
                        ColumnDef::new(NativeBalance::LastChangedTxIndex)
                            .big_integer()
                            .not_null()
                            .default(0),
                    )
                    .col(
                        ColumnDef::new(NativeBalance::LastChangedLogIndex)
                            .big_integer()
                            .not_null()
                            .default(0),
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
