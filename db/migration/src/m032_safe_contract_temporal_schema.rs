use sea_orm_migration::prelude::*;

/// Migration to convert hopr_safe_contract to a temporal table pattern.
///
/// This migration:
/// 1. Creates hopr_safe_contract_state table for mutable state (module_address, chain_key)
/// 2. Migrates existing data from hopr_safe_contract to the new state table
/// 3. Removes mutable columns from hopr_safe_contract (keeping only identity: id, address)
///
/// The temporal pattern allows:
/// - Tracking module address changes over time (e.g., Safe module migrations)
/// - Point-in-time queries for historical state
/// - Preserving pre-seeded data separately from indexed data
#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let backend = manager.get_database_backend();

        // For SQLite, we need a different approach due to table restructuring limitations.
        // We create the state table WITHOUT a FK first, restructure the identity table,
        // then recreate state table with FK.
        match backend {
            sea_orm::DatabaseBackend::Sqlite => self.up_sqlite(manager).await,
            sea_orm::DatabaseBackend::Postgres => self.up_postgres(manager).await,
            _ => Err(DbErr::Custom("Unsupported database backend".to_string())),
        }
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let backend = manager.get_database_backend();

        // Drop the view first
        manager
            .get_connection()
            .execute_unprepared("DROP VIEW IF EXISTS safe_contract_current")
            .await?;

        // Recreate the original hopr_safe_contract table structure
        match backend {
            sea_orm::DatabaseBackend::Sqlite => {
                // Create new table with all original columns
                manager
                    .get_connection()
                    .execute_unprepared(
                        "CREATE TABLE hopr_safe_contract_old (
                            id INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
                            address BLOB(20) NOT NULL UNIQUE,
                            module_address BLOB(20) NOT NULL,
                            chain_key BLOB(20) NOT NULL,
                            deployed_block INTEGER NOT NULL,
                            deployed_tx_index INTEGER NOT NULL,
                            deployed_log_index INTEGER NOT NULL
                        )",
                    )
                    .await?;

                // Migrate data back from state table (using latest state for each safe)
                manager
                    .get_connection()
                    .execute_unprepared(
                        "INSERT INTO hopr_safe_contract_old (id, address, module_address, chain_key, deployed_block, deployed_tx_index, deployed_log_index)
                         SELECT sc.id, sc.address, scs.module_address, scs.chain_key, scs.published_block, scs.published_tx_index, scs.published_log_index
                         FROM hopr_safe_contract sc
                         JOIN hopr_safe_contract_state scs ON scs.hopr_safe_contract_id = sc.id
                         WHERE scs.id = (
                             SELECT s2.id FROM hopr_safe_contract_state s2
                             WHERE s2.hopr_safe_contract_id = sc.id
                             ORDER BY s2.published_block DESC, s2.published_tx_index DESC, s2.published_log_index DESC
                             LIMIT 1
                         )",
                    )
                    .await?;

                // Drop the state table and current table
                manager
                    .drop_table(Table::drop().table(HoprSafeContractState::Table).to_owned())
                    .await?;

                manager
                    .get_connection()
                    .execute_unprepared("DROP TABLE hopr_safe_contract")
                    .await?;

                manager
                    .get_connection()
                    .execute_unprepared("ALTER TABLE hopr_safe_contract_old RENAME TO hopr_safe_contract")
                    .await?;
            }
            sea_orm::DatabaseBackend::Postgres => {
                // Add back the columns
                manager
                    .alter_table(
                        Table::alter()
                            .table(HoprSafeContract::Table)
                            .add_column(
                                ColumnDef::new(HoprSafeContract::ModuleAddress)
                                    .binary_len(20)
                                    .not_null()
                                    .default(vec![0u8; 20]),
                            )
                            .to_owned(),
                    )
                    .await?;

                manager
                    .alter_table(
                        Table::alter()
                            .table(HoprSafeContract::Table)
                            .add_column(
                                ColumnDef::new(HoprSafeContract::ChainKey)
                                    .binary_len(20)
                                    .not_null()
                                    .default(vec![0u8; 20]),
                            )
                            .to_owned(),
                    )
                    .await?;

                manager
                    .alter_table(
                        Table::alter()
                            .table(HoprSafeContract::Table)
                            .add_column(
                                ColumnDef::new(HoprSafeContract::DeployedBlock)
                                    .big_integer()
                                    .not_null()
                                    .default(0),
                            )
                            .to_owned(),
                    )
                    .await?;

                manager
                    .alter_table(
                        Table::alter()
                            .table(HoprSafeContract::Table)
                            .add_column(
                                ColumnDef::new(HoprSafeContract::DeployedTxIndex)
                                    .big_integer()
                                    .not_null()
                                    .default(0),
                            )
                            .to_owned(),
                    )
                    .await?;

                manager
                    .alter_table(
                        Table::alter()
                            .table(HoprSafeContract::Table)
                            .add_column(
                                ColumnDef::new(HoprSafeContract::DeployedLogIndex)
                                    .big_integer()
                                    .not_null()
                                    .default(0),
                            )
                            .to_owned(),
                    )
                    .await?;

                // Migrate data back
                manager
                    .get_connection()
                    .execute_unprepared(
                        "UPDATE hopr_safe_contract sc
                         SET module_address = scs.module_address,
                             chain_key = scs.chain_key,
                             deployed_block = scs.published_block,
                             deployed_tx_index = scs.published_tx_index,
                             deployed_log_index = scs.published_log_index
                         FROM hopr_safe_contract_state scs
                         WHERE scs.hopr_safe_contract_id = sc.id
                         AND scs.id = (
                             SELECT s2.id FROM hopr_safe_contract_state s2
                             WHERE s2.hopr_safe_contract_id = sc.id
                             ORDER BY s2.published_block DESC, s2.published_tx_index DESC, s2.published_log_index DESC
                             LIMIT 1
                         )",
                    )
                    .await?;

                // Drop the state table
                manager
                    .drop_table(Table::drop().table(HoprSafeContractState::Table).to_owned())
                    .await?;
            }
            _ => return Err(DbErr::Custom("Unsupported database backend".to_string())),
        }

        // Recreate original indices
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

impl Migration {
    /// SQLite migration: table restructuring approach
    ///
    /// SQLite doesn't support DROP COLUMN, so we must recreate tables.
    /// Order of operations is critical:
    /// 1. Create temporary state table (no FK) to hold migrated data
    /// 2. Drop all indices on identity table
    /// 3. Recreate identity table with only id + address
    /// 4. Drop temp state table
    /// 5. Create final state table with FK
    /// 6. Migrate data back to final state table
    async fn up_sqlite(&self, manager: &SchemaManager<'_>) -> Result<(), DbErr> {
        let conn = manager.get_connection();

        // Step 1: Create temporary state table WITHOUT foreign key
        conn.execute_unprepared(
            "CREATE TABLE hopr_safe_contract_state_temp (
                id INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
                hopr_safe_contract_id INTEGER NOT NULL,
                module_address BLOB(20) NOT NULL,
                chain_key BLOB(20) NOT NULL,
                published_block INTEGER NOT NULL,
                published_tx_index INTEGER NOT NULL,
                published_log_index INTEGER NOT NULL
            )",
        )
        .await?;

        // Step 2: Migrate data from old table to temp state table
        conn.execute_unprepared(
            "INSERT INTO hopr_safe_contract_state_temp (hopr_safe_contract_id, module_address, chain_key, published_block, published_tx_index, published_log_index)
             SELECT id, module_address, chain_key, deployed_block, deployed_tx_index, deployed_log_index
             FROM hopr_safe_contract",
        )
        .await?;

        // Step 3: Drop ALL indices on the identity table
        conn.execute_unprepared("DROP INDEX IF EXISTS idx_hopr_safe_contract_event")
            .await?;
        conn.execute_unprepared("DROP INDEX IF EXISTS idx_hopr_safe_contract_address_module_address")
            .await?;
        conn.execute_unprepared("DROP INDEX IF EXISTS idx_hopr_safe_contract_address")
            .await?;
        conn.execute_unprepared("DROP INDEX IF EXISTS idx_hopr_safe_contract_module_address")
            .await?;
        conn.execute_unprepared("DROP INDEX IF EXISTS idx_hopr_safe_contract_chain_key")
            .await?;
        // Also drop any SQLite auto-generated unique index
        conn.execute_unprepared("DROP INDEX IF EXISTS sqlite_autoindex_hopr_safe_contract_1")
            .await?;

        // Step 4: Restructure identity table (SQLite table rebuild)
        conn.execute_unprepared(
            "CREATE TABLE hopr_safe_contract_new (
                id INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
                address BLOB(20) NOT NULL UNIQUE
            )",
        )
        .await?;

        conn.execute_unprepared(
            "INSERT INTO hopr_safe_contract_new (id, address) SELECT id, address FROM hopr_safe_contract",
        )
        .await?;

        conn.execute_unprepared("DROP TABLE hopr_safe_contract")
            .await?;

        conn.execute_unprepared("ALTER TABLE hopr_safe_contract_new RENAME TO hopr_safe_contract")
            .await?;

        // Step 5: Create final state table WITH foreign key
        conn.execute_unprepared(
            "CREATE TABLE hopr_safe_contract_state (
                id INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
                hopr_safe_contract_id INTEGER NOT NULL,
                module_address BLOB(20) NOT NULL,
                chain_key BLOB(20) NOT NULL,
                published_block INTEGER NOT NULL,
                published_tx_index INTEGER NOT NULL,
                published_log_index INTEGER NOT NULL,
                FOREIGN KEY (hopr_safe_contract_id) REFERENCES hopr_safe_contract (id) ON DELETE CASCADE ON UPDATE CASCADE
            )",
        )
        .await?;

        // Step 6: Migrate data from temp to final state table
        conn.execute_unprepared(
            "INSERT INTO hopr_safe_contract_state (hopr_safe_contract_id, module_address, chain_key, published_block, published_tx_index, published_log_index)
             SELECT hopr_safe_contract_id, module_address, chain_key, published_block, published_tx_index, published_log_index
             FROM hopr_safe_contract_state_temp",
        )
        .await?;

        // Step 7: Drop temp table
        conn.execute_unprepared("DROP TABLE hopr_safe_contract_state_temp")
            .await?;

        // Step 8: Create indices on state table
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

        // Step 9: Create view for current state
        conn.execute_unprepared(
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
            )",
        )
        .await?;

        Ok(())
    }

    /// PostgreSQL migration: standard ALTER TABLE approach
    async fn up_postgres(&self, manager: &SchemaManager<'_>) -> Result<(), DbErr> {
        // Step 1: Create hopr_safe_contract_state table with FK
        manager
            .create_table(
                Table::create()
                    .table(HoprSafeContractState::Table)
                    .if_not_exists()
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
                            .name("fk_safe_contract_state_safe_contract_id")
                            .from(HoprSafeContractState::Table, HoprSafeContractState::HoprSafeContractId)
                            .to(HoprSafeContract::Table, HoprSafeContract::Id)
                            .on_delete(ForeignKeyAction::Cascade)
                            .on_update(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        // Step 2: Create indices
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

        // Step 3: Migrate existing data
        manager
            .get_connection()
            .execute_unprepared(
                "INSERT INTO hopr_safe_contract_state (hopr_safe_contract_id, module_address, chain_key, published_block, published_tx_index, published_log_index)
                 SELECT id, module_address, chain_key, deployed_block, deployed_tx_index, deployed_log_index
                 FROM hopr_safe_contract",
            )
            .await?;

        // Step 4: Drop old indices
        manager
            .drop_index(
                Index::drop()
                    .name("idx_hopr_safe_contract_event")
                    .table(HoprSafeContract::Table)
                    .to_owned(),
            )
            .await
            .ok();

        manager
            .drop_index(
                Index::drop()
                    .name("idx_hopr_safe_contract_address_module_address")
                    .table(HoprSafeContract::Table)
                    .to_owned(),
            )
            .await
            .ok();

        manager
            .drop_index(
                Index::drop()
                    .name("idx_hopr_safe_contract_module_address")
                    .table(HoprSafeContract::Table)
                    .to_owned(),
            )
            .await
            .ok();

        manager
            .drop_index(
                Index::drop()
                    .name("idx_hopr_safe_contract_chain_key")
                    .table(HoprSafeContract::Table)
                    .to_owned(),
            )
            .await
            .ok();

        // Step 5: Drop columns from identity table
        manager
            .alter_table(
                Table::alter()
                    .table(HoprSafeContract::Table)
                    .drop_column(HoprSafeContract::ModuleAddress)
                    .to_owned(),
            )
            .await?;

        manager
            .alter_table(
                Table::alter()
                    .table(HoprSafeContract::Table)
                    .drop_column(HoprSafeContract::ChainKey)
                    .to_owned(),
            )
            .await?;

        manager
            .alter_table(
                Table::alter()
                    .table(HoprSafeContract::Table)
                    .drop_column(HoprSafeContract::DeployedBlock)
                    .to_owned(),
            )
            .await?;

        manager
            .alter_table(
                Table::alter()
                    .table(HoprSafeContract::Table)
                    .drop_column(HoprSafeContract::DeployedTxIndex)
                    .to_owned(),
            )
            .await?;

        manager
            .alter_table(
                Table::alter()
                    .table(HoprSafeContract::Table)
                    .drop_column(HoprSafeContract::DeployedLogIndex)
                    .to_owned(),
            )
            .await?;

        // Step 6: Create view for current state
        manager
            .get_connection()
            .execute_unprepared(
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
                )",
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
