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
                    .table(HoprSafeActivity::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(HoprSafeActivity::Id)
                            .big_integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(HoprSafeActivity::HoprSafeContractId)
                            .big_integer()
                            .not_null(),
                    )
                    .col(ColumnDef::new(HoprSafeActivity::EventKind).string().not_null())
                    .col(ColumnDef::new(HoprSafeActivity::ChainTxHash).binary_len(32).not_null())
                    .col(ColumnDef::new(HoprSafeActivity::SafeTxHash).binary_len(32))
                    .col(ColumnDef::new(HoprSafeActivity::OwnerAddress).binary_len(20))
                    .col(ColumnDef::new(HoprSafeActivity::Threshold).string())
                    .col(ColumnDef::new(HoprSafeActivity::Payment).string())
                    .col(ColumnDef::new(HoprSafeActivity::InitiatorAddress).binary_len(20))
                    .col(
                        ColumnDef::new(HoprSafeActivity::PublishedBlock)
                            .big_integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(HoprSafeActivity::PublishedTxIndex)
                            .big_integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(HoprSafeActivity::PublishedLogIndex)
                            .big_integer()
                            .not_null(),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .from(HoprSafeActivity::Table, HoprSafeActivity::HoprSafeContractId)
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
                    .name("idx_safe_activity_unique_position")
                    .table(HoprSafeActivity::Table)
                    .col(HoprSafeActivity::HoprSafeContractId)
                    .col(HoprSafeActivity::PublishedBlock)
                    .col(HoprSafeActivity::PublishedTxIndex)
                    .col(HoprSafeActivity::PublishedLogIndex)
                    .unique()
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_safe_activity_safe_position")
                    .table(HoprSafeActivity::Table)
                    .col(HoprSafeActivity::HoprSafeContractId)
                    .col((HoprSafeActivity::PublishedBlock, IndexOrder::Desc))
                    .col((HoprSafeActivity::PublishedTxIndex, IndexOrder::Desc))
                    .col((HoprSafeActivity::PublishedLogIndex, IndexOrder::Desc))
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_safe_activity_chain_tx_hash")
                    .table(HoprSafeActivity::Table)
                    .col(HoprSafeActivity::ChainTxHash)
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

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared("DROP VIEW IF EXISTS safe_owner_current")
            .await?;

        manager
            .drop_table(Table::drop().table(HoprSafeActivity::Table).if_exists().to_owned())
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
enum HoprSafeActivity {
    Table,
    Id,
    HoprSafeContractId,
    EventKind,
    ChainTxHash,
    SafeTxHash,
    OwnerAddress,
    Threshold,
    Payment,
    InitiatorAddress,
    PublishedBlock,
    PublishedTxIndex,
    PublishedLogIndex,
}
