use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared("DROP VIEW IF EXISTS safe_contract_current")
            .await?;

        manager
            .drop_table(Table::drop().table(HoprSafeContractState::Table).if_exists().to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(HoprSafeContract::Table).if_exists().to_owned())
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(HoprSafeContract::Table)
                    .col(
                        ColumnDef::new(HoprSafeContract::Id)
                            .big_integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(HoprSafeContract::Address)
                            .binary_len(20)
                            .not_null()
                            .unique_key(),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(HoprSafeContractState::Table)
                    .col(
                        ColumnDef::new(HoprSafeContractState::Id)
                            .big_integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(HoprSafeContractState::HoprSafeContractId)
                            .big_integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(HoprSafeContractState::ModuleAddress)
                            .binary_len(20)
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(HoprSafeContractState::ChainKey)
                            .binary_len(20)
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(HoprSafeContractState::PublishedBlock)
                            .big_integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(HoprSafeContractState::PublishedTxIndex)
                            .big_integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(HoprSafeContractState::PublishedLogIndex)
                            .big_integer()
                            .not_null(),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .from(HoprSafeContractState::Table, HoprSafeContractState::HoprSafeContractId)
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
                    .name("idx_safe_contract_state_unique_position")
                    .table(HoprSafeContractState::Table)
                    .col(HoprSafeContractState::HoprSafeContractId)
                    .col(HoprSafeContractState::PublishedBlock)
                    .col(HoprSafeContractState::PublishedTxIndex)
                    .col(HoprSafeContractState::PublishedLogIndex)
                    .unique()
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_safe_contract_state_position")
                    .table(HoprSafeContractState::Table)
                    .col(HoprSafeContractState::HoprSafeContractId)
                    .col((HoprSafeContractState::PublishedBlock, IndexOrder::Desc))
                    .col((HoprSafeContractState::PublishedTxIndex, IndexOrder::Desc))
                    .col((HoprSafeContractState::PublishedLogIndex, IndexOrder::Desc))
                    .to_owned(),
            )
            .await?;

        let backend = manager.get_database_backend();
        let view_sql = match backend {
            sea_orm::DatabaseBackend::Postgres => {
                "CREATE OR REPLACE VIEW safe_contract_current AS
                SELECT
                    sc.id AS safe_contract_id,
                    sc.address,
                    scs.module_address,
                    scs.chain_key,
                    scs.published_block,
                    scs.published_tx_index,
                    scs.published_log_index
                FROM hopr_safe_contract sc
                JOIN hopr_safe_contract_state scs ON scs.hopr_safe_contract_id = sc.id
                WHERE scs.id = (
                    SELECT s2.id FROM hopr_safe_contract_state s2
                    WHERE s2.hopr_safe_contract_id = sc.id
                    ORDER BY s2.published_block DESC, s2.published_tx_index DESC, s2.published_log_index DESC
                    LIMIT 1
                )"
            }
            _ => {
                "CREATE VIEW IF NOT EXISTS safe_contract_current AS
                SELECT
                    sc.id AS safe_contract_id,
                    sc.address,
                    scs.module_address,
                    scs.chain_key,
                    scs.published_block,
                    scs.published_tx_index,
                    scs.published_log_index
                FROM hopr_safe_contract sc
                JOIN hopr_safe_contract_state scs ON scs.hopr_safe_contract_id = sc.id
                WHERE scs.id = (
                    SELECT s2.id FROM hopr_safe_contract_state s2
                    WHERE s2.hopr_safe_contract_id = sc.id
                    ORDER BY s2.published_block DESC, s2.published_tx_index DESC, s2.published_log_index DESC
                    LIMIT 1
                )"
            }
        };

        manager.get_connection().execute_unprepared(view_sql).await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared("DROP VIEW IF EXISTS safe_contract_current")
            .await?;

        manager
            .drop_table(Table::drop().table(HoprSafeContractState::Table).if_exists().to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(HoprSafeContract::Table).if_exists().to_owned())
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(HoprSafeContract::Table)
                    .col(
                        ColumnDef::new(HoprSafeContract::Id)
                            .big_integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(HoprSafeContract::Address)
                            .binary_len(20)
                            .not_null()
                            .unique_key(),
                    )
                    .col(
                        ColumnDef::new(HoprSafeContract::ModuleAddress)
                            .binary_len(20)
                            .not_null(),
                    )
                    .col(ColumnDef::new(HoprSafeContract::ChainKey).binary_len(20).not_null())
                    .col(ColumnDef::new(HoprSafeContract::DeployedBlock).big_integer().not_null())
                    .col(
                        ColumnDef::new(HoprSafeContract::DeployedTxIndex)
                            .big_integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(HoprSafeContract::DeployedLogIndex)
                            .big_integer()
                            .not_null(),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_hopr_safe_contract_event")
                    .table(HoprSafeContract::Table)
                    .col(HoprSafeContract::DeployedBlock)
                    .col(HoprSafeContract::DeployedTxIndex)
                    .col(HoprSafeContract::DeployedLogIndex)
                    .unique()
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_hopr_safe_contract_address_module_address")
                    .table(HoprSafeContract::Table)
                    .col(HoprSafeContract::Address)
                    .col(HoprSafeContract::ModuleAddress)
                    .unique()
                    .to_owned(),
            )
            .await?;

        Ok(())
    }
}

#[derive(DeriveIden)]
enum HoprSafeContract {
    Table,
    Id,
    Address,
    ModuleAddress,
    ChainKey,
    DeployedBlock,
    DeployedTxIndex,
    DeployedLogIndex,
}

#[derive(DeriveIden)]
enum HoprSafeContractState {
    Table,
    Id,
    HoprSafeContractId,
    ModuleAddress,
    ChainKey,
    PublishedBlock,
    PublishedTxIndex,
    PublishedLogIndex,
}
