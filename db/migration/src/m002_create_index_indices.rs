use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Channel indices
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

        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx_channel_source_destination")
                    .table(Channel::Table)
                    .col(Channel::Source)
                    .col(Channel::Destination)
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx_channel_id_channel_epoch")
                    .table(Channel::Table)
                    .col(Channel::ChannelId)
                    .col(Channel::Epoch)
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx_channel_closure_time")
                    .table(Channel::Table)
                    .col(Channel::ClosureTime)
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx_channel_status")
                    .table(Channel::Table)
                    .col(Channel::Status)
                    .to_owned(),
            )
            .await?;

        // Account indices
        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx_account_chain_key")
                    .table(Account::Table)
                    .col(Account::ChainKey)
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx_account_packet_key")
                    .table(Account::Table)
                    .col(Account::PacketKey)
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx_account_chain_packet_key")
                    .table(Account::Table)
                    .col(Account::ChainKey)
                    .col(Account::PacketKey)
                    .unique()
                    .to_owned(),
            )
            .await?;

        // Announcement indices
        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx_announcement_account_id")
                    .table(Announcement::Table)
                    .col(Announcement::AccountId)
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Drop all indices
        manager
            .drop_index(Index::drop().name("idx_announcement_account_id").to_owned())
            .await?;
        manager
            .drop_index(
                Index::drop()
                    .name("idx_account_chain_packet_key")
                    .to_owned(),
            )
            .await?;
        manager
            .drop_index(Index::drop().name("idx_account_packet_key").to_owned())
            .await?;
        manager
            .drop_index(Index::drop().name("idx_account_chain_key").to_owned())
            .await?;
        manager
            .drop_index(Index::drop().name("idx_channel_status").to_owned())
            .await?;
        manager
            .drop_index(Index::drop().name("idx_channel_closure_time").to_owned())
            .await?;
        manager
            .drop_index(
                Index::drop()
                    .name("idx_channel_id_channel_epoch")
                    .to_owned(),
            )
            .await?;
        manager
            .drop_index(
                Index::drop()
                    .name("idx_channel_source_destination")
                    .to_owned(),
            )
            .await?;
        manager
            .drop_index(Index::drop().name("idx_channel_source").to_owned())
            .await?;
        manager
            .drop_index(Index::drop().name("idx_channel_destination").to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum Channel {
    Table,
    ChannelId,
    Source,
    Destination,
    Epoch,
    ClosureTime,
    Status,
}

#[derive(DeriveIden)]
enum Account {
    Table,
    ChainKey,
    PacketKey,
}

#[derive(DeriveIden)]
enum Announcement {
    Table,
    AccountId,
}
