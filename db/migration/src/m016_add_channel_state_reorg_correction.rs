use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Add reorg_correction flag to channel_state table
        // This field marks states that were inserted as corrections after a blockchain reorganization
        // Unlike corrupted_state (which has a different meaning), reorg_correction specifically indicates
        // that this state was created to fix history after a reorg event
        manager
            .alter_table(
                Table::alter()
                    .table(ChannelState::Table)
                    .add_column(
                        ColumnDef::new(ChannelState::ReorgCorrection)
                            .boolean()
                            .not_null()
                            .default(false),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Remove reorg_correction flag from channel_state table
        manager
            .alter_table(
                Table::alter()
                    .table(ChannelState::Table)
                    .drop_column(ChannelState::ReorgCorrection)
                    .to_owned(),
            )
            .await
    }
}

#[derive(DeriveIden)]
enum ChannelState {
    Table,
    ReorgCorrection,
}
