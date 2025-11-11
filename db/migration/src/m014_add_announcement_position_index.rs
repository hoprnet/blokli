use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Add index for temporal announcement queries
        // Enables efficient "announcements at or before position" queries
        manager
            .create_index(
                Index::create()
                    .name("idx_announcement_position")
                    .table(Announcement::Table)
                    .col(Announcement::AccountId)
                    .col((Announcement::PublishedBlock, IndexOrder::Desc))
                    .col((Announcement::PublishedTxIndex, IndexOrder::Desc))
                    .col((Announcement::PublishedLogIndex, IndexOrder::Desc))
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_index(
                Index::drop()
                    .name("idx_announcement_position")
                    .table(Announcement::Table)
                    .to_owned(),
            )
            .await
    }
}

#[derive(DeriveIden)]
enum Announcement {
    Table,
    AccountId,
    PublishedBlock,
    PublishedTxIndex,
    PublishedLogIndex,
}
