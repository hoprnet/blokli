use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        create_pending_note_table(manager).await?;
        create_batch_item_table(
            manager,
            CurvyCommittedNote::Table,
            CurvyCommittedNote::Id,
            CurvyCommittedNote::BatchIndex,
            CurvyCommittedNote::NoteId,
            CurvyCommittedNote::EventItemIndex,
            CurvyCommittedNote::ChainTxHash,
            CurvyCommittedNote::BlockHash,
            CurvyCommittedNote::PublishedBlock,
            CurvyCommittedNote::PublishedTxIndex,
            CurvyCommittedNote::PublishedLogIndex,
            CurvyCommittedNote::LeafIndex,
            "idx_curvy_committed_note_unique_position",
            "idx_curvy_committed_note_leaf_index",
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
            CurvyCommittedNullifier::BlockHash,
            CurvyCommittedNullifier::PublishedBlock,
            CurvyCommittedNullifier::PublishedTxIndex,
            CurvyCommittedNullifier::PublishedLogIndex,
            CurvyCommittedNullifier::NullifierIndex,
            "idx_curvy_committed_nullifier_unique_position",
            "idx_curvy_committed_nullifier_index",
        )
        .await?;

        for index in [
            Index::create()
                .name("idx_curvy_pending_note_note_id")
                .table(CurvyPendingNote::Table)
                .col(CurvyPendingNote::NoteId)
                .to_owned(),
            Index::create()
                .name("idx_curvy_committed_note_note_id")
                .table(CurvyCommittedNote::Table)
                .col(CurvyCommittedNote::NoteId)
                .to_owned(),
        ] {
            manager.create_index(index).await?;
        }

        manager
            .create_table(
                Table::create()
                    .table(CurvyShardRoot::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(CurvyShardRoot::Id)
                            .big_integer()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(CurvyShardRoot::TreeVersion).integer().not_null())
                    .col(ColumnDef::new(CurvyShardRoot::ShardHeight).integer().not_null())
                    .col(ColumnDef::new(CurvyShardRoot::ShardIndex).big_integer().not_null())
                    .col(ColumnDef::new(CurvyShardRoot::Root).binary_len(32).not_null())
                    .col(ColumnDef::new(CurvyShardRoot::BlockHash).binary_len(32).not_null())
                    .col(ColumnDef::new(CurvyShardRoot::ChainTxHash).binary_len(32).not_null())
                    .col(ColumnDef::new(CurvyShardRoot::CompletionBlock).big_integer().not_null())
                    .col(
                        ColumnDef::new(CurvyShardRoot::CompletionTxIndex)
                            .big_integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(CurvyShardRoot::CompletionLogIndex)
                            .big_integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(CurvyShardRoot::CompletionEventItemIndex)
                            .big_integer()
                            .not_null(),
                    )
                    .index(
                        Index::create()
                            .name("idx_curvy_shard_root_geometry_index")
                            .col(CurvyShardRoot::TreeVersion)
                            .col(CurvyShardRoot::ShardHeight)
                            .col(CurvyShardRoot::ShardIndex)
                            .unique(),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(CurvySyncCheckpoint::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(CurvySyncCheckpoint::Id)
                            .big_integer()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(CurvySyncCheckpoint::BlockNumber)
                            .big_integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(CurvySyncCheckpoint::BlockHash)
                            .binary_len(32)
                            .not_null()
                            .unique_key(),
                    )
                    .col(
                        ColumnDef::new(CurvySyncCheckpoint::AggregatorAddress)
                            .binary_len(20)
                            .not_null(),
                    )
                    .col(ColumnDef::new(CurvySyncCheckpoint::TreeVersion).integer().not_null())
                    .col(ColumnDef::new(CurvySyncCheckpoint::TreeDepth).integer().not_null())
                    .col(ColumnDef::new(CurvySyncCheckpoint::ShardHeight).integer().not_null())
                    .col(ColumnDef::new(CurvySyncCheckpoint::LeafCount).big_integer().not_null())
                    .col(
                        ColumnDef::new(CurvySyncCheckpoint::NullifierCount)
                            .big_integer()
                            .not_null(),
                    )
                    .col(ColumnDef::new(CurvySyncCheckpoint::ShardCount).big_integer().not_null())
                    .col(ColumnDef::new(CurvySyncCheckpoint::Root).binary_len(32).not_null())
                    .col(ColumnDef::new(CurvySyncCheckpoint::FrontierSnapshot).blob().not_null())
                    .index(
                        Index::create()
                            .name("idx_curvy_sync_checkpoint_block")
                            .col(CurvySyncCheckpoint::BlockNumber)
                            .unique(),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        for table in [
            CurvySyncCheckpoint::Table.to_string(),
            CurvyShardRoot::Table.to_string(),
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

async fn create_pending_note_table(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
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
                .col(ColumnDef::new(CurvyPendingNote::BlockHash).binary_len(32).not_null())
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
        .await
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
    block_hash: T,
    published_block: T,
    published_tx_index: T,
    published_log_index: T,
    dense_index: T,
    position_index_name: &str,
    dense_index_name: &str,
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
                .col(ColumnDef::new(block_hash).binary_len(32).not_null())
                .col(ColumnDef::new(published_block.clone()).big_integer().not_null())
                .col(ColumnDef::new(published_tx_index.clone()).big_integer().not_null())
                .col(ColumnDef::new(published_log_index.clone()).big_integer().not_null())
                .col(ColumnDef::new(dense_index.clone()).big_integer().not_null())
                .index(
                    Index::create()
                        .name(position_index_name)
                        .col(published_block)
                        .col(published_tx_index)
                        .col(published_log_index)
                        .col(event_item_index)
                        .unique(),
                )
                .index(Index::create().name(dense_index_name).col(dense_index).unique())
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
    BlockHash,
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
    BlockHash,
    PublishedBlock,
    PublishedTxIndex,
    PublishedLogIndex,
    LeafIndex,
}

#[derive(DeriveIden, Clone)]
enum CurvyCommittedNullifier {
    Table,
    Id,
    BatchIndex,
    Nullifier,
    EventItemIndex,
    ChainTxHash,
    BlockHash,
    PublishedBlock,
    PublishedTxIndex,
    PublishedLogIndex,
    NullifierIndex,
}

#[derive(DeriveIden)]
enum CurvyShardRoot {
    Table,
    Id,
    TreeVersion,
    ShardHeight,
    ShardIndex,
    Root,
    BlockHash,
    ChainTxHash,
    CompletionBlock,
    CompletionTxIndex,
    CompletionLogIndex,
    CompletionEventItemIndex,
}

#[derive(DeriveIden)]
enum CurvySyncCheckpoint {
    Table,
    Id,
    BlockNumber,
    BlockHash,
    AggregatorAddress,
    TreeVersion,
    TreeDepth,
    ShardHeight,
    LeafCount,
    NullifierCount,
    ShardCount,
    Root,
    FrontierSnapshot,
}
