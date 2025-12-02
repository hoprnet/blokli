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

        // Clear chain_info and node_info
        manager
            .exec_stmt(Query::delete().from_table(ChainInfo::Table).to_owned())
            .await?;

        manager
            .exec_stmt(Query::delete().from_table(NodeInfo::Table).to_owned())
            .await?;

        // Seed initial ChainInfo entry with default values
        manager
            .exec_stmt(
                Query::insert()
                    .into_table(ChainInfo::Table)
                    .columns([ChainInfo::Id])
                    .values_panic([1.into()])
                    .to_owned(),
            )
            .await?;

        // Seed initial NodeInfo entry with default values
        manager
            .exec_stmt(
                Query::insert()
                    .into_table(NodeInfo::Table)
                    .columns([NodeInfo::Id])
                    .values_panic([1.into()])
                    .to_owned(),
            )
            .await
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
}

#[derive(DeriveIden)]
enum NodeInfo {
    Table,
    Id,
}

#[derive(DeriveIden)]
enum HoprSafeContract {
    Table,
}
