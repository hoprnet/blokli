use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(CurvyPendingNote::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(CurvyPendingNote::Id)
                            .big_integer()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(CurvyPendingNote::NoteId).binary_len(32).not_null())
                    .col(
                        ColumnDef::new(CurvyPendingNote::EphemeralKeyX)
                            .binary_len(32)
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(CurvyPendingNote::EphemeralKeyY)
                            .binary_len(32)
                            .not_null(),
                    )
                    .col(ColumnDef::new(CurvyPendingNote::ViewTag).integer().not_null())
                    .col(ColumnDef::new(CurvyPendingNote::TokenId).binary_len(32).not_null())
                    .col(ColumnDef::new(CurvyPendingNote::Amount).binary_len(32).not_null())
                    .col(ColumnDef::new(CurvyPendingNote::IsPlaintext).boolean().not_null())
                    .col(ColumnDef::new(CurvyPendingNote::EventItemIndex).integer().not_null())
                    .col(ColumnDef::new(CurvyPendingNote::ChainTxHash).binary_len(32).not_null())
                    .col(
                        ColumnDef::new(CurvyPendingNote::PublishedBlock)
                            .big_integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(CurvyPendingNote::PublishedTxIndex)
                            .big_integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(CurvyPendingNote::PublishedLogIndex)
                            .big_integer()
                            .not_null(),
                    )
                    .index(
                        Index::create()
                            .name("idx_curvy_pending_note_unique_position")
                            .col(CurvyPendingNote::PublishedBlock)
                            .col(CurvyPendingNote::PublishedTxIndex)
                            .col(CurvyPendingNote::PublishedLogIndex)
                            .col(CurvyPendingNote::EventItemIndex)
                            .unique(),
                    )
                    .to_owned(),
            )
            .await?;

        create_batch_item_table(
            manager,
            CurvyCommittedNote::Table,
            CurvyCommittedNote::Id,
            CurvyCommittedNote::BatchIndex,
            CurvyCommittedNote::NoteId,
            CurvyCommittedNote::EventItemIndex,
            CurvyCommittedNote::ChainTxHash,
            CurvyCommittedNote::PublishedBlock,
            CurvyCommittedNote::PublishedTxIndex,
            CurvyCommittedNote::PublishedLogIndex,
            "idx_curvy_committed_note_unique_position",
        )
        .await?;
        create_batch_item_table(
            manager,
            CurvyCommittedNullifier::Table,
            CurvyCommittedNullifier::Id,
            CurvyCommittedNullifier::BatchIndex,
            CurvyCommittedNullifier::Nullifier,
            CurvyCommittedNullifier::EventItemIndex,
            CurvyCommittedNullifier::ChainTxHash,
            CurvyCommittedNullifier::PublishedBlock,
            CurvyCommittedNullifier::PublishedTxIndex,
            CurvyCommittedNullifier::PublishedLogIndex,
            "idx_curvy_committed_nullifier_unique_position",
        )
        .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        for table in [
            CurvyCommittedNullifier::Table.to_string(),
            CurvyCommittedNote::Table.to_string(),
            CurvyPendingNote::Table.to_string(),
        ] {
            manager
                .drop_table(Table::drop().table(Alias::new(table)).to_owned())
                .await?;
        }
        Ok(())
    }
}

#[allow(clippy::too_many_arguments)]
async fn create_batch_item_table<T: Iden + Clone + 'static>(
    manager: &SchemaManager<'_>,
    table: T,
    id: T,
    batch_index: T,
    item_value: T,
    event_item_index: T,
    chain_tx_hash: T,
    published_block: T,
    published_tx_index: T,
    published_log_index: T,
    index_name: &str,
) -> Result<(), DbErr> {
    manager
        .create_table(
            Table::create()
                .table(table)
                .if_not_exists()
                .col(ColumnDef::new(id).big_integer().auto_increment().primary_key())
                .col(ColumnDef::new(batch_index).binary_len(32).not_null())
                .col(ColumnDef::new(item_value).binary_len(32).not_null())
                .col(ColumnDef::new(event_item_index.clone()).integer().not_null())
                .col(ColumnDef::new(chain_tx_hash).binary_len(32).not_null())
                .col(ColumnDef::new(published_block.clone()).big_integer().not_null())
                .col(ColumnDef::new(published_tx_index.clone()).big_integer().not_null())
                .col(ColumnDef::new(published_log_index.clone()).big_integer().not_null())
                .index(
                    Index::create()
                        .name(index_name)
                        .col(published_block)
                        .col(published_tx_index)
                        .col(published_log_index)
                        .col(event_item_index)
                        .unique(),
                )
                .to_owned(),
        )
        .await
}

#[derive(DeriveIden)]
enum CurvyPendingNote {
    Table,
    Id,
    NoteId,
    EphemeralKeyX,
    EphemeralKeyY,
    ViewTag,
    TokenId,
    Amount,
    IsPlaintext,
    EventItemIndex,
    ChainTxHash,
    PublishedBlock,
    PublishedTxIndex,
    PublishedLogIndex,
}
#[derive(DeriveIden, Clone)]
enum CurvyCommittedNote {
    Table,
    Id,
    BatchIndex,
    NoteId,
    EventItemIndex,
    ChainTxHash,
    PublishedBlock,
    PublishedTxIndex,
    PublishedLogIndex,
}
#[derive(DeriveIden, Clone)]
enum CurvyCommittedNullifier {
    Table,
    Id,
    BatchIndex,
    Nullifier,
    EventItemIndex,
    ChainTxHash,
    PublishedBlock,
    PublishedTxIndex,
    PublishedLogIndex,
}
