use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(HoprSafeRedeemedStatEvent::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(HoprSafeRedeemedStatEvent::Id)
                            .big_integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(HoprSafeRedeemedStatEvent::SafeAddress)
                            .binary_len(20)
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(HoprSafeRedeemedStatEvent::NodeAddress)
                            .binary_len(20)
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(HoprSafeRedeemedStatEvent::RedeemedAmount)
                            .binary_len(32)
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(HoprSafeRedeemedStatEvent::PublishedBlock)
                            .big_integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(HoprSafeRedeemedStatEvent::PublishedTxIndex)
                            .big_integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(HoprSafeRedeemedStatEvent::PublishedLogIndex)
                            .big_integer()
                            .not_null(),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_safe_redeemed_stat_event_unique_position")
                    .table(HoprSafeRedeemedStatEvent::Table)
                    .col(HoprSafeRedeemedStatEvent::SafeAddress)
                    .col(HoprSafeRedeemedStatEvent::NodeAddress)
                    .col(HoprSafeRedeemedStatEvent::PublishedBlock)
                    .col(HoprSafeRedeemedStatEvent::PublishedTxIndex)
                    .col(HoprSafeRedeemedStatEvent::PublishedLogIndex)
                    .unique()
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_safe_redeemed_stat_event_safe_node")
                    .table(HoprSafeRedeemedStatEvent::Table)
                    .col(HoprSafeRedeemedStatEvent::SafeAddress)
                    .col(HoprSafeRedeemedStatEvent::NodeAddress)
                    .to_owned(),
            )
            .await?;

        manager
            .drop_index(
                Index::drop()
                    .name("idx_channel_state_status_position")
                    .table(ChannelState::Table)
                    .if_exists()
                    .to_owned(),
            )
            .await?;

        manager
            .drop_index(
                Index::drop()
                    .name("idx_channel_state_status_channel_position")
                    .table(ChannelState::Table)
                    .if_exists()
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_index(
                Index::create()
                    .name("idx_channel_state_status_position")
                    .table(ChannelState::Table)
                    .col(ChannelState::Status)
                    .col((ChannelState::PublishedBlock, IndexOrder::Desc))
                    .col((ChannelState::PublishedTxIndex, IndexOrder::Desc))
                    .col((ChannelState::PublishedLogIndex, IndexOrder::Desc))
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_channel_state_status_channel_position")
                    .table(ChannelState::Table)
                    .col(ChannelState::Status)
                    .col(ChannelState::ChannelId)
                    .col((ChannelState::PublishedBlock, IndexOrder::Desc))
                    .col((ChannelState::PublishedTxIndex, IndexOrder::Desc))
                    .col((ChannelState::PublishedLogIndex, IndexOrder::Desc))
                    .to_owned(),
            )
            .await?;

        manager
            .drop_table(
                Table::drop()
                    .table(HoprSafeRedeemedStatEvent::Table)
                    .if_exists()
                    .to_owned(),
            )
            .await?;

        Ok(())
    }
}

#[derive(DeriveIden)]
enum HoprSafeRedeemedStatEvent {
    Table,
    Id,
    SafeAddress,
    NodeAddress,
    RedeemedAmount,
    PublishedBlock,
    PublishedTxIndex,
    PublishedLogIndex,
}

#[derive(DeriveIden)]
enum ChannelState {
    Table,
    Status,
    ChannelId,
    PublishedBlock,
    PublishedTxIndex,
    PublishedLogIndex,
}
