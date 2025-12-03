use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Clear index data from tables, respecting foreign key relationships
        // Delete in order of dependencies

        // Clear hopr_safe_contract (no dependencies)
        manager
            .exec_stmt(Query::delete().from_table(HoprSafeContract::Table).to_owned())
            .await?;

        // Clear state history tables (depend on channel and account)
        manager
            .exec_stmt(Query::delete().from_table(ChannelState::Table).to_owned())
            .await?;

        manager
            .exec_stmt(Query::delete().from_table(AccountState::Table).to_owned())
            .await?;

        // Clear balance tables (depend on account)
        manager
            .exec_stmt(Query::delete().from_table(HoprBalance::Table).to_owned())
            .await?;

        manager
            .exec_stmt(Query::delete().from_table(NativeBalance::Table).to_owned())
            .await?;

        // Clear relational tables (depend on account)
        manager
            .exec_stmt(Query::delete().from_table(Announcement::Table).to_owned())
            .await?;

        manager
            .exec_stmt(Query::delete().from_table(Channel::Table).to_owned())
            .await?;

        // Clear base entity
        manager
            .exec_stmt(Query::delete().from_table(Account::Table).to_owned())
            .await?;

        // Drop and recreate chain_info and node_info without auto_increment
        manager
            .drop_table(Table::drop().table(ChainInfo::Table).to_owned())
            .await?;

        manager
            .drop_table(Table::drop().table(NodeInfo::Table).to_owned())
            .await?;

        // Recreate NodeInfo table without auto_increment
        manager
            .create_table(
                Table::create()
                    .table(NodeInfo::Table)
                    .if_not_exists()
                    .col(ColumnDef::new(NodeInfo::Id).big_integer().not_null().primary_key())
                    .col(
                        ColumnDef::new(NodeInfo::SafeBalance)
                            .binary_len(12)
                            .not_null()
                            .default(vec![0u8; 12]),
                    )
                    .col(
                        ColumnDef::new(NodeInfo::SafeAllowance)
                            .binary_len(12)
                            .not_null()
                            .default(vec![0u8; 12]),
                    )
                    .col(ColumnDef::new(NodeInfo::SafeAddress).binary_len(20).null())
                    .col(ColumnDef::new(NodeInfo::ModuleAddress).binary_len(20).null())
                    .to_owned(),
            )
            .await?;

        // Recreate ChainInfo table without auto_increment
        manager
            .create_table(
                Table::create()
                    .table(ChainInfo::Table)
                    .if_not_exists()
                    .col(ColumnDef::new(ChainInfo::Id).big_integer().not_null().primary_key())
                    .col(
                        ColumnDef::new(ChainInfo::LastIndexedBlock)
                            .big_integer()
                            .not_null()
                            .default(0),
                    )
                    .col(ColumnDef::new(ChainInfo::LastIndexedTxIndex).big_integer().null())
                    .col(ColumnDef::new(ChainInfo::LastIndexedLogIndex).big_integer().null())
                    .col(ColumnDef::new(ChainInfo::TicketPrice).binary_len(12).null())
                    .col(ColumnDef::new(ChainInfo::ChannelsDST).binary_len(32).null())
                    .col(ColumnDef::new(ChainInfo::LedgerDST).binary_len(32).null())
                    .col(ColumnDef::new(ChainInfo::SafeRegistryDST).binary_len(32).null())
                    .col(
                        ColumnDef::new(ChainInfo::MinIncomingTicketWinProb)
                            .float()
                            .not_null()
                            .default(1.0),
                    )
                    .col(
                        ColumnDef::new(ChainInfo::ChannelClosureGracePeriod)
                            .big_integer()
                            .null(),
                    )
                    .col(ColumnDef::new(ChainInfo::KeyBindingFee).binary_len(12).null())
                    .to_owned(),
            )
            .await?;

        // Note: ChainInfo and NodeInfo seeding moved to application startup
        // See bloklid/src/main.rs for initialization logic
        Ok(())
    }

    async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        // Down migration is a no-op for data reset
        // Schema is preserved, so no action needed to reverse
        Ok(())
    }
}

#[derive(DeriveIden)]
enum Account {
    Table,
}

#[derive(DeriveIden)]
enum Announcement {
    Table,
}

#[derive(DeriveIden)]
enum Channel {
    Table,
}

#[derive(DeriveIden)]
enum ChannelState {
    Table,
}

#[derive(DeriveIden)]
enum AccountState {
    Table,
}

#[derive(DeriveIden)]
enum HoprBalance {
    Table,
}

#[derive(DeriveIden)]
enum NativeBalance {
    Table,
}

#[derive(DeriveIden)]
enum ChainInfo {
    Table,
    Id,
    LastIndexedBlock,
    LastIndexedTxIndex,
    LastIndexedLogIndex,
    TicketPrice,
    ChannelsDST,
    LedgerDST,
    SafeRegistryDST,
    MinIncomingTicketWinProb,
    ChannelClosureGracePeriod,
    KeyBindingFee,
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

#[derive(DeriveIden)]
enum HoprSafeContract {
    Table,
}
