use sea_orm_migration::prelude::*;

/// Replaces the Safe event history with a current-state model.
///
/// Safe owners and threshold are no longer rebuilt from Safe event logs. They come from an RPC
/// snapshot and are kept current by live events, so:
///
/// * `hopr_safe_owner_state` keeps one row per `(safe, owner)` (removed owners stay as `is_current_owner = false` so a
///   replayed `AddedOwner` cannot revive them),
/// * `hopr_safe_threshold_state` keeps one row per Safe,
/// * `hopr_safe_contract_state.owners_synced_block` records the block the owner snapshot is pinned at; live owner and
///   threshold events at or below it are ignored,
/// * the six Safe event history tables, which no production code reads, are dropped.
///
/// Rows that predate this migration are reduced to the latest row per key.
#[derive(DeriveMigrationName)]
pub struct Migration;

const POSITION_NEWER_THAN_SELF: &str = "(later.published_block, later.published_tx_index, later.published_log_index) \
                                        > (t.published_block, t.published_tx_index, t.published_log_index)";

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let conn = manager.get_connection();
        let create_view = if manager.get_database_backend() == sea_orm::DatabaseBackend::Postgres {
            "CREATE OR REPLACE VIEW"
        } else {
            "CREATE VIEW IF NOT EXISTS"
        };

        conn.execute_unprepared("DROP VIEW IF EXISTS safe_threshold_current")
            .await?;
        conn.execute_unprepared("DROP VIEW IF EXISTS safe_owner_current")
            .await?;

        conn.execute_unprepared("ALTER TABLE hopr_safe_contract_state ADD COLUMN owners_synced_block BIGINT")
            .await?;

        // Keep only the latest row per (safe, owner) and per safe.
        conn.execute_unprepared(&format!(
            "DELETE FROM hopr_safe_owner_state WHERE id IN (SELECT t.id FROM hopr_safe_owner_state AS t WHERE EXISTS \
             (SELECT 1 FROM hopr_safe_owner_state AS later WHERE later.hopr_safe_contract_id = \
             t.hopr_safe_contract_id AND later.owner_address = t.owner_address AND {POSITION_NEWER_THAN_SELF}))"
        ))
        .await?;
        conn.execute_unprepared(&format!(
            "DELETE FROM hopr_safe_threshold_state WHERE id IN (SELECT t.id FROM hopr_safe_threshold_state AS t WHERE \
             EXISTS (SELECT 1 FROM hopr_safe_threshold_state AS later WHERE later.hopr_safe_contract_id = \
             t.hopr_safe_contract_id AND {POSITION_NEWER_THAN_SELF}))"
        ))
        .await?;

        for index in [
            "idx_safe_owner_state_unique_position",
            "idx_safe_owner_state_current_lookup",
            "idx_safe_owner_state_owner_lookup",
            "idx_safe_threshold_state_unique_position",
            "idx_safe_threshold_state_current_lookup",
        ] {
            conn.execute_unprepared(&format!("DROP INDEX IF EXISTS {index}"))
                .await?;
        }

        conn.execute_unprepared(
            "CREATE UNIQUE INDEX IF NOT EXISTS idx_safe_owner_state_unique_owner ON hopr_safe_owner_state (hopr_safe_contract_id, \
             owner_address)",
        )
        .await?;
        conn.execute_unprepared(
            "CREATE INDEX IF NOT EXISTS idx_safe_owner_state_by_owner ON hopr_safe_owner_state (owner_address)",
        )
        .await?;
        conn.execute_unprepared(
            "CREATE UNIQUE INDEX IF NOT EXISTS idx_safe_threshold_state_unique_safe ON hopr_safe_threshold_state \
             (hopr_safe_contract_id)",
        )
        .await?;

        conn.execute_unprepared(&format!(
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
            WHERE sos.is_current_owner = TRUE"
        ))
        .await?;
        conn.execute_unprepared(&format!(
            "{create_view} safe_threshold_current AS
            SELECT
                sc.id AS safe_contract_id,
                sc.address AS safe_address,
                sts.threshold,
                sts.published_block,
                sts.published_tx_index,
                sts.published_log_index
            FROM hopr_safe_contract sc
            JOIN hopr_safe_threshold_state sts ON sts.hopr_safe_contract_id = sc.id"
        ))
        .await?;

        for table in [
            "hopr_safe_setup_owner",
            "hopr_safe_setup_event",
            "hopr_safe_owner_change_event",
            "hopr_safe_threshold_change_event",
            "hopr_safe_execution_event",
            "hopr_safe_event",
        ] {
            conn.execute_unprepared(&format!("DROP TABLE IF EXISTS {table}"))
                .await?;
        }

        Ok(())
    }

    async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        Err(DbErr::Migration(
            "the Safe event history tables are dropped for good; this migration cannot be reverted".to_string(),
        ))
    }
}
