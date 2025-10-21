use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Index for channel source lookups
        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx_channel_source")
                    .table(Channel::Table)
                    .col(Channel::Source)
                    .to_owned(),
            )
            .await?;

        // Index for channel destination lookups
        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx_channel_destination")
                    .table(Channel::Table)
                    .col(Channel::Destination)
                    .to_owned(),
            )
            .await?;

        // Index for HoprBalance filtering by last changed block
        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx_hopr_balance_last_changed_block")
                    .table(HoprBalance::Table)
                    .col(HoprBalance::LastChangedBlock)
                    .to_owned(),
            )
            .await?;

        // Index for NativeBalance filtering by last changed block
        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx_native_balance_last_changed_block")
                    .table(NativeBalance::Table)
                    .col(NativeBalance::LastChangedBlock)
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_index(Index::drop().name("idx_native_balance_last_changed_block").to_owned())
            .await?;
        manager
            .drop_index(Index::drop().name("idx_hopr_balance_last_changed_block").to_owned())
            .await?;
        manager
            .drop_index(Index::drop().name("idx_channel_destination").to_owned())
            .await?;
        manager
            .drop_index(Index::drop().name("idx_channel_source").to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum Channel {
    Table,
    Source,
    Destination,
}

#[derive(DeriveIden)]
enum HoprBalance {
    Table,
    LastChangedBlock,
}

#[derive(DeriveIden)]
enum NativeBalance {
    Table,
    LastChangedBlock,
}
