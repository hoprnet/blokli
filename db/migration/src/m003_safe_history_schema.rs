use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(HoprSafeOwnerState::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(HoprSafeOwnerState::Id)
                            .big_integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(HoprSafeOwnerState::HoprSafeContractId)
                            .big_integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(HoprSafeOwnerState::OwnerAddress)
                            .binary_len(20)
                            .not_null(),
                    )
                    .col(ColumnDef::new(HoprSafeOwnerState::IsCurrentOwner).boolean().not_null())
                    .col(
                        ColumnDef::new(HoprSafeOwnerState::PublishedBlock)
                            .big_integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(HoprSafeOwnerState::PublishedTxIndex)
                            .big_integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(HoprSafeOwnerState::PublishedLogIndex)
                            .big_integer()
                            .not_null(),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .from(HoprSafeOwnerState::Table, HoprSafeOwnerState::HoprSafeContractId)
                            .to(HoprSafeContract::Table, HoprSafeContract::Id)
                            .on_delete(ForeignKeyAction::Cascade)
                            .on_update(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_safe_owner_state_unique_position")
                    .table(HoprSafeOwnerState::Table)
                    .col(HoprSafeOwnerState::HoprSafeContractId)
                    .col(HoprSafeOwnerState::OwnerAddress)
                    .col(HoprSafeOwnerState::PublishedBlock)
                    .col(HoprSafeOwnerState::PublishedTxIndex)
                    .col(HoprSafeOwnerState::PublishedLogIndex)
                    .unique()
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_safe_owner_state_current_lookup")
                    .table(HoprSafeOwnerState::Table)
                    .col(HoprSafeOwnerState::HoprSafeContractId)
                    .col(HoprSafeOwnerState::OwnerAddress)
                    .col((HoprSafeOwnerState::PublishedBlock, IndexOrder::Desc))
                    .col((HoprSafeOwnerState::PublishedTxIndex, IndexOrder::Desc))
                    .col((HoprSafeOwnerState::PublishedLogIndex, IndexOrder::Desc))
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_safe_owner_state_owner_lookup")
                    .table(HoprSafeOwnerState::Table)
                    .col(HoprSafeOwnerState::OwnerAddress)
                    .col((HoprSafeOwnerState::PublishedBlock, IndexOrder::Desc))
                    .col((HoprSafeOwnerState::PublishedTxIndex, IndexOrder::Desc))
                    .col((HoprSafeOwnerState::PublishedLogIndex, IndexOrder::Desc))
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(HoprSafeEvent::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(HoprSafeEvent::Id)
                            .big_integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(HoprSafeEvent::HoprSafeContractId)
                            .big_integer()
                            .not_null(),
                    )
                    .col(ColumnDef::new(HoprSafeEvent::EventKind).string().not_null())
                    .col(ColumnDef::new(HoprSafeEvent::ChainTxHash).binary_len(32).not_null())
                    .col(ColumnDef::new(HoprSafeEvent::PublishedBlock).big_integer().not_null())
                    .col(ColumnDef::new(HoprSafeEvent::PublishedTxIndex).big_integer().not_null())
                    .col(
                        ColumnDef::new(HoprSafeEvent::PublishedLogIndex)
                            .big_integer()
                            .not_null(),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .from(HoprSafeEvent::Table, HoprSafeEvent::HoprSafeContractId)
                            .to(HoprSafeContract::Table, HoprSafeContract::Id)
                            .on_delete(ForeignKeyAction::Cascade)
                            .on_update(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_safe_event_unique_position")
                    .table(HoprSafeEvent::Table)
                    .col(HoprSafeEvent::HoprSafeContractId)
                    .col(HoprSafeEvent::PublishedBlock)
                    .col(HoprSafeEvent::PublishedTxIndex)
                    .col(HoprSafeEvent::PublishedLogIndex)
                    .unique()
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_safe_event_safe_position")
                    .table(HoprSafeEvent::Table)
                    .col(HoprSafeEvent::HoprSafeContractId)
                    .col((HoprSafeEvent::PublishedBlock, IndexOrder::Desc))
                    .col((HoprSafeEvent::PublishedTxIndex, IndexOrder::Desc))
                    .col((HoprSafeEvent::PublishedLogIndex, IndexOrder::Desc))
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_safe_event_chain_tx_hash")
                    .table(HoprSafeEvent::Table)
                    .col(HoprSafeEvent::ChainTxHash)
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(HoprSafeSetupEvent::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(HoprSafeSetupEvent::HoprSafeEventId)
                            .big_integer()
                            .not_null()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(HoprSafeSetupEvent::InitiatorAddress).binary_len(20))
                    .col(ColumnDef::new(HoprSafeSetupEvent::Threshold).string().not_null())
                    .foreign_key(
                        ForeignKey::create()
                            .from(HoprSafeSetupEvent::Table, HoprSafeSetupEvent::HoprSafeEventId)
                            .to(HoprSafeEvent::Table, HoprSafeEvent::Id)
                            .on_delete(ForeignKeyAction::Cascade)
                            .on_update(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(HoprSafeSetupOwner::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(HoprSafeSetupOwner::Id)
                            .big_integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(HoprSafeSetupOwner::HoprSafeEventId)
                            .big_integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(HoprSafeSetupOwner::OwnerPosition)
                            .big_integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(HoprSafeSetupOwner::OwnerAddress)
                            .binary_len(20)
                            .not_null(),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .from(HoprSafeSetupOwner::Table, HoprSafeSetupOwner::HoprSafeEventId)
                            .to(HoprSafeSetupEvent::Table, HoprSafeSetupEvent::HoprSafeEventId)
                            .on_delete(ForeignKeyAction::Cascade)
                            .on_update(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_safe_setup_owner_unique_position")
                    .table(HoprSafeSetupOwner::Table)
                    .col(HoprSafeSetupOwner::HoprSafeEventId)
                    .col(HoprSafeSetupOwner::OwnerPosition)
                    .unique()
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_safe_setup_owner_event_lookup")
                    .table(HoprSafeSetupOwner::Table)
                    .col(HoprSafeSetupOwner::HoprSafeEventId)
                    .col(HoprSafeSetupOwner::OwnerPosition)
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(HoprSafeOwnerChangeEvent::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(HoprSafeOwnerChangeEvent::HoprSafeEventId)
                            .big_integer()
                            .not_null()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(HoprSafeOwnerChangeEvent::OwnerAddress)
                            .binary_len(20)
                            .not_null(),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .from(
                                HoprSafeOwnerChangeEvent::Table,
                                HoprSafeOwnerChangeEvent::HoprSafeEventId,
                            )
                            .to(HoprSafeEvent::Table, HoprSafeEvent::Id)
                            .on_delete(ForeignKeyAction::Cascade)
                            .on_update(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(HoprSafeThresholdChangeEvent::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(HoprSafeThresholdChangeEvent::HoprSafeEventId)
                            .big_integer()
                            .not_null()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(HoprSafeThresholdChangeEvent::Threshold)
                            .string()
                            .not_null(),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .from(
                                HoprSafeThresholdChangeEvent::Table,
                                HoprSafeThresholdChangeEvent::HoprSafeEventId,
                            )
                            .to(HoprSafeEvent::Table, HoprSafeEvent::Id)
                            .on_delete(ForeignKeyAction::Cascade)
                            .on_update(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(HoprSafeExecutionEvent::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(HoprSafeExecutionEvent::HoprSafeEventId)
                            .big_integer()
                            .not_null()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(HoprSafeExecutionEvent::SafeTxHash)
                            .binary_len(32)
                            .not_null(),
                    )
                    .col(ColumnDef::new(HoprSafeExecutionEvent::Payment).string().not_null())
                    .foreign_key(
                        ForeignKey::create()
                            .from(HoprSafeExecutionEvent::Table, HoprSafeExecutionEvent::HoprSafeEventId)
                            .to(HoprSafeEvent::Table, HoprSafeEvent::Id)
                            .on_delete(ForeignKeyAction::Cascade)
                            .on_update(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_safe_execution_safe_tx_hash")
                    .table(HoprSafeExecutionEvent::Table)
                    .col(HoprSafeExecutionEvent::SafeTxHash)
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(HoprSafeThresholdState::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(HoprSafeThresholdState::Id)
                            .big_integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(HoprSafeThresholdState::HoprSafeContractId)
                            .big_integer()
                            .not_null(),
                    )
                    .col(ColumnDef::new(HoprSafeThresholdState::Threshold).string().not_null())
                    .col(
                        ColumnDef::new(HoprSafeThresholdState::PublishedBlock)
                            .big_integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(HoprSafeThresholdState::PublishedTxIndex)
                            .big_integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(HoprSafeThresholdState::PublishedLogIndex)
                            .big_integer()
                            .not_null(),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .from(
                                HoprSafeThresholdState::Table,
                                HoprSafeThresholdState::HoprSafeContractId,
                            )
                            .to(HoprSafeContract::Table, HoprSafeContract::Id)
                            .on_delete(ForeignKeyAction::Cascade)
                            .on_update(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_safe_threshold_state_unique_position")
                    .table(HoprSafeThresholdState::Table)
                    .col(HoprSafeThresholdState::HoprSafeContractId)
                    .col(HoprSafeThresholdState::PublishedBlock)
                    .col(HoprSafeThresholdState::PublishedTxIndex)
                    .col(HoprSafeThresholdState::PublishedLogIndex)
                    .unique()
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_safe_threshold_state_current_lookup")
                    .table(HoprSafeThresholdState::Table)
                    .col(HoprSafeThresholdState::HoprSafeContractId)
                    .col((HoprSafeThresholdState::PublishedBlock, IndexOrder::Desc))
                    .col((HoprSafeThresholdState::PublishedTxIndex, IndexOrder::Desc))
                    .col((HoprSafeThresholdState::PublishedLogIndex, IndexOrder::Desc))
                    .to_owned(),
            )
            .await?;

        let create_view = if manager.get_database_backend() == sea_orm::DatabaseBackend::Postgres {
            "CREATE OR REPLACE VIEW"
        } else {
            "CREATE VIEW IF NOT EXISTS"
        };

        manager
            .get_connection()
            .execute_unprepared(&format!(
                "{create_view} safe_owner_current AS
                SELECT
                    sc.id AS safe_contract_id,
                    sc.address AS safe_address,
                    sos.owner_address,
                    sos.published_block,
                    sos.published_tx_index,
                    sos.published_log_index
                FROM hopr_safe_contract sc
                JOIN hopr_safe_owner_state sos ON sos.hopr_safe_contract_id = sc.id
                WHERE sos.is_current_owner = TRUE
                  AND sos.id = (
                    SELECT s2.id FROM hopr_safe_owner_state s2
                    WHERE s2.hopr_safe_contract_id = sc.id
                      AND s2.owner_address = sos.owner_address
                    ORDER BY s2.published_block DESC, s2.published_tx_index DESC, s2.published_log_index DESC
                    LIMIT 1
                )"
            ))
            .await?;

        manager
            .get_connection()
            .execute_unprepared(&format!(
                "{create_view} safe_threshold_current AS
                SELECT
                    sc.id AS safe_contract_id,
                    sc.address AS safe_address,
                    sts.threshold,
                    sts.published_block,
                    sts.published_tx_index,
                    sts.published_log_index
                FROM hopr_safe_contract sc
                JOIN hopr_safe_threshold_state sts ON sts.hopr_safe_contract_id = sc.id
                WHERE sts.id = (
                    SELECT s2.id FROM hopr_safe_threshold_state s2
                    WHERE s2.hopr_safe_contract_id = sc.id
                    ORDER BY s2.published_block DESC, s2.published_tx_index DESC, s2.published_log_index DESC
                    LIMIT 1
                )"
            ))
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared("DROP VIEW IF EXISTS safe_threshold_current")
            .await?;

        manager
            .get_connection()
            .execute_unprepared("DROP VIEW IF EXISTS safe_owner_current")
            .await?;

        manager
            .drop_table(
                Table::drop()
                    .table(HoprSafeThresholdState::Table)
                    .if_exists()
                    .to_owned(),
            )
            .await?;

        manager
            .drop_table(
                Table::drop()
                    .table(HoprSafeExecutionEvent::Table)
                    .if_exists()
                    .to_owned(),
            )
            .await?;

        manager
            .drop_table(
                Table::drop()
                    .table(HoprSafeThresholdChangeEvent::Table)
                    .if_exists()
                    .to_owned(),
            )
            .await?;

        manager
            .drop_table(
                Table::drop()
                    .table(HoprSafeOwnerChangeEvent::Table)
                    .if_exists()
                    .to_owned(),
            )
            .await?;

        manager
            .drop_table(Table::drop().table(HoprSafeSetupOwner::Table).if_exists().to_owned())
            .await?;

        manager
            .drop_table(Table::drop().table(HoprSafeSetupEvent::Table).if_exists().to_owned())
            .await?;

        manager
            .drop_table(Table::drop().table(HoprSafeEvent::Table).if_exists().to_owned())
            .await?;

        manager
            .drop_table(Table::drop().table(HoprSafeOwnerState::Table).if_exists().to_owned())
            .await?;

        Ok(())
    }
}

#[derive(DeriveIden)]
enum HoprSafeContract {
    Table,
    Id,
}

#[derive(DeriveIden)]
enum HoprSafeOwnerState {
    Table,
    Id,
    HoprSafeContractId,
    OwnerAddress,
    IsCurrentOwner,
    PublishedBlock,
    PublishedTxIndex,
    PublishedLogIndex,
}

#[derive(DeriveIden)]
enum HoprSafeEvent {
    Table,
    Id,
    HoprSafeContractId,
    EventKind,
    ChainTxHash,
    PublishedBlock,
    PublishedTxIndex,
    PublishedLogIndex,
}

#[derive(DeriveIden)]
enum HoprSafeSetupEvent {
    Table,
    HoprSafeEventId,
    InitiatorAddress,
    Threshold,
}

#[derive(DeriveIden)]
enum HoprSafeSetupOwner {
    Table,
    Id,
    HoprSafeEventId,
    OwnerPosition,
    OwnerAddress,
}

#[derive(DeriveIden)]
enum HoprSafeOwnerChangeEvent {
    Table,
    HoprSafeEventId,
    OwnerAddress,
}

#[derive(DeriveIden)]
enum HoprSafeThresholdChangeEvent {
    Table,
    HoprSafeEventId,
    Threshold,
}

#[derive(DeriveIden)]
enum HoprSafeExecutionEvent {
    Table,
    HoprSafeEventId,
    SafeTxHash,
    Payment,
}

#[derive(DeriveIden)]
enum HoprSafeThresholdState {
    Table,
    Id,
    HoprSafeContractId,
    Threshold,
    PublishedBlock,
    PublishedTxIndex,
    PublishedLogIndex,
}
