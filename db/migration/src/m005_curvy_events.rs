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
            CurvyCommittedNote::Value,
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
            CurvyCommittedNullifier::Value,
            CurvyCommittedNullifier::EventItemIndex,
            CurvyCommittedNullifier::ChainTxHash,
            CurvyCommittedNullifier::PublishedBlock,
            CurvyCommittedNullifier::PublishedTxIndex,
            CurvyCommittedNullifier::PublishedLogIndex,
            "idx_curvy_committed_nullifier_unique_position",
        )
        .await?;

        manager
            .create_table(
                Table::create()
                    .table(CurvyCommitmentGasFeeRoot::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(CurvyCommitmentGasFeeRoot::Id)
                            .big_integer()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(CurvyCommitmentGasFeeRoot::Root)
                            .binary_len(32)
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(CurvyCommitmentGasFeeRoot::ChainTxHash)
                            .binary_len(32)
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(CurvyCommitmentGasFeeRoot::PublishedBlock)
                            .big_integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(CurvyCommitmentGasFeeRoot::PublishedTxIndex)
                            .big_integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(CurvyCommitmentGasFeeRoot::PublishedLogIndex)
                            .big_integer()
                            .not_null(),
                    )
                    .index(&mut position_index(
                        "idx_curvy_commitment_gas_fee_root_unique_position",
                        CurvyCommitmentGasFeeRoot::PublishedBlock,
                        CurvyCommitmentGasFeeRoot::PublishedTxIndex,
                        CurvyCommitmentGasFeeRoot::PublishedLogIndex,
                    ))
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(CurvyTokenRegistration::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(CurvyTokenRegistration::Id)
                            .big_integer()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(CurvyTokenRegistration::TokenAddress)
                            .binary_len(20)
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(CurvyTokenRegistration::TokenId)
                            .binary_len(32)
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(CurvyTokenRegistration::ChainTxHash)
                            .binary_len(32)
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(CurvyTokenRegistration::PublishedBlock)
                            .big_integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(CurvyTokenRegistration::PublishedTxIndex)
                            .big_integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(CurvyTokenRegistration::PublishedLogIndex)
                            .big_integer()
                            .not_null(),
                    )
                    .index(&mut position_index(
                        "idx_curvy_token_registration_unique_position",
                        CurvyTokenRegistration::PublishedBlock,
                        CurvyTokenRegistration::PublishedTxIndex,
                        CurvyTokenRegistration::PublishedLogIndex,
                    ))
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(CurvyCommitmentGasCost::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(CurvyCommitmentGasCost::Id)
                            .big_integer()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(CurvyCommitmentGasCost::TokenId)
                            .binary_len(32)
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(CurvyCommitmentGasCost::PortalDeployment)
                            .binary_len(32)
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(CurvyCommitmentGasCost::PendingNoteCommitment)
                            .binary_len(32)
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(CurvyCommitmentGasCost::Withdrawal)
                            .binary_len(32)
                            .not_null(),
                    )
                    .col(ColumnDef::new(CurvyCommitmentGasCost::Root).binary_len(32).not_null())
                    .col(
                        ColumnDef::new(CurvyCommitmentGasCost::EventItemIndex)
                            .integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(CurvyCommitmentGasCost::ChainTxHash)
                            .binary_len(32)
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(CurvyCommitmentGasCost::PublishedBlock)
                            .big_integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(CurvyCommitmentGasCost::PublishedTxIndex)
                            .big_integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(CurvyCommitmentGasCost::PublishedLogIndex)
                            .big_integer()
                            .not_null(),
                    )
                    .index(
                        Index::create()
                            .name("idx_curvy_commitment_gas_cost_unique_position")
                            .col(CurvyCommitmentGasCost::PublishedBlock)
                            .col(CurvyCommitmentGasCost::PublishedTxIndex)
                            .col(CurvyCommitmentGasCost::PublishedLogIndex)
                            .col(CurvyCommitmentGasCost::EventItemIndex)
                            .unique(),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        for table in [
            CurvyCommitmentGasCost::Table.to_string(),
            CurvyTokenRegistration::Table.to_string(),
            CurvyCommitmentGasFeeRoot::Table.to_string(),
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
    value: T,
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
                .col(ColumnDef::new(value).binary_len(32).not_null())
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

fn position_index<T: Iden + 'static>(name: &str, block: T, tx_index: T, log_index: T) -> IndexCreateStatement {
    Index::create()
        .name(name)
        .col(block)
        .col(tx_index)
        .col(log_index)
        .unique()
        .to_owned()
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
    Value,
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
    Value,
    EventItemIndex,
    ChainTxHash,
    PublishedBlock,
    PublishedTxIndex,
    PublishedLogIndex,
}
#[derive(DeriveIden)]
enum CurvyCommitmentGasFeeRoot {
    Table,
    Id,
    Root,
    ChainTxHash,
    PublishedBlock,
    PublishedTxIndex,
    PublishedLogIndex,
}
#[derive(DeriveIden)]
enum CurvyTokenRegistration {
    Table,
    Id,
    TokenAddress,
    TokenId,
    ChainTxHash,
    PublishedBlock,
    PublishedTxIndex,
    PublishedLogIndex,
}
#[derive(DeriveIden)]
enum CurvyCommitmentGasCost {
    Table,
    Id,
    TokenId,
    PortalDeployment,
    PendingNoteCommitment,
    Withdrawal,
    Root,
    EventItemIndex,
    ChainTxHash,
    PublishedBlock,
    PublishedTxIndex,
    PublishedLogIndex,
}
