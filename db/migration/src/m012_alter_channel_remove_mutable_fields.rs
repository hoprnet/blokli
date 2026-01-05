use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Drop indexes that reference columns we're about to remove
        // Ignore errors if indexes don't exist (fresh database case)
        let _ = manager
            .drop_index(Index::drop().name("idx_channel_status").to_owned())
            .await;

        let _ = manager
            .drop_index(Index::drop().name("idx_channel_id_channel_epoch").to_owned())
            .await;

        let _ = manager
            .drop_index(Index::drop().name("idx_channel_closure_time").to_owned())
            .await;

        // Drop mutable columns from channel table
        // These fields are now stored in channel_state for full version history
        // Ignore errors if columns don't exist (fresh database case)
        let _ = manager
            .alter_table(
                Table::alter()
                    .table(Channel::Table)
                    .drop_column(Channel::Balance)
                    .to_owned(),
            )
            .await;

        let _ = manager
            .alter_table(
                Table::alter()
                    .table(Channel::Table)
                    .drop_column(Channel::Status)
                    .to_owned(),
            )
            .await;

        let _ = manager
            .alter_table(
                Table::alter()
                    .table(Channel::Table)
                    .drop_column(Channel::Epoch)
                    .to_owned(),
            )
            .await;

        let _ = manager
            .alter_table(
                Table::alter()
                    .table(Channel::Table)
                    .drop_column(Channel::TicketIndex)
                    .to_owned(),
            )
            .await;

        let _ = manager
            .alter_table(
                Table::alter()
                    .table(Channel::Table)
                    .drop_column(Channel::ClosureTime)
                    .to_owned(),
            )
            .await;

        let _ = manager
            .alter_table(
                Table::alter()
                    .table(Channel::Table)
                    .drop_column(Channel::CorruptedState)
                    .to_owned(),
            )
            .await;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Restore mutable columns
        manager
            .alter_table(
                Table::alter()
                    .table(Channel::Table)
                    .add_column(ColumnDef::new(Channel::Balance).binary_len(12).not_null())
                    .to_owned(),
            )
            .await?;

        manager
            .alter_table(
                Table::alter()
                    .table(Channel::Table)
                    .add_column(ColumnDef::new(Channel::Status).small_integer().not_null())
                    .to_owned(),
            )
            .await?;

        manager
            .alter_table(
                Table::alter()
                    .table(Channel::Table)
                    .add_column(ColumnDef::new(Channel::Epoch).big_integer().not_null().default(1))
                    .to_owned(),
            )
            .await?;

        manager
            .alter_table(
                Table::alter()
                    .table(Channel::Table)
                    .add_column(ColumnDef::new(Channel::TicketIndex).big_integer().not_null().default(0))
                    .to_owned(),
            )
            .await?;

        manager
            .alter_table(
                Table::alter()
                    .table(Channel::Table)
                    .add_column(ColumnDef::new(Channel::ClosureTime).timestamp().null())
                    .to_owned(),
            )
            .await?;

        manager
            .alter_table(
                Table::alter()
                    .table(Channel::Table)
                    .add_column(
                        ColumnDef::new(Channel::CorruptedState)
                            .boolean()
                            .not_null()
                            .default(false),
                    )
                    .to_owned(),
            )
            .await?;

        // Restore the indexes
        manager
            .create_index(
                Index::create()
                    .name("idx_channel_status")
                    .table(Channel::Table)
                    .col(Channel::Status)
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_channel_id_channel_epoch")
                    .table(Channel::Table)
                    .col(Channel::ConcreteChannelId)
                    .col(Channel::Epoch)
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_channel_closure_time")
                    .table(Channel::Table)
                    .col(Channel::ClosureTime)
                    .to_owned(),
            )
            .await
    }
}

#[derive(DeriveIden)]
enum Channel {
    Table,
    ConcreteChannelId,
    Balance,
    Status,
    Epoch,
    TicketIndex,
    ClosureTime,
    CorruptedState,
}
