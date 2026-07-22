use sea_orm_migration::prelude::*;

/// Rewrites the `channel_current` and `account_current` views to use a
/// correlated `ORDER BY ... LIMIT 1` subquery instead of a
/// `ROW_NUMBER() OVER (PARTITION BY ...)` window function.
///
/// The window-function form acts as an optimization fence: a predicate such as
/// `WHERE concrete_channel_id = $1` cannot be pushed into the windowed subquery,
/// so PostgreSQL scans and sorts the *entire* `channel_state` / `account_state`
/// table on every single-row lookup. As those tables grow this shows up as
/// multi-second "slow statement" warnings.
///
/// The correlated form (mirroring `safe_contract_current`) lets the planner
/// resolve the parent row via its unique index and satisfy the subquery with an
/// index scan on `idx_channel_state_position` / `idx_account_state_position`,
/// turning full-table sorts into O(log n) lookups. It is portable across
/// PostgreSQL and SQLite.
#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let backend = manager.get_database_backend();

        // SQLite does not support `CREATE OR REPLACE VIEW`, so drop first.
        if backend != sea_orm::DatabaseBackend::Postgres {
            manager
                .get_connection()
                .execute_unprepared("DROP VIEW IF EXISTS channel_current")
                .await?;
            manager
                .get_connection()
                .execute_unprepared("DROP VIEW IF EXISTS account_current")
                .await?;
        }

        let create_view = if backend == sea_orm::DatabaseBackend::Postgres {
            "CREATE OR REPLACE VIEW"
        } else {
            "CREATE VIEW IF NOT EXISTS"
        };

        manager
            .get_connection()
            .execute_unprepared(&format!(
                "{create_view} channel_current AS
                SELECT
                    cs.id,
                    c.id AS channel_id,
                    c.concrete_channel_id,
                    c.source,
                    c.destination,
                    cs.balance,
                    cs.status,
                    cs.epoch,
                    cs.ticket_index,
                    cs.closure_time,
                    cs.corrupted_state,
                    cs.published_block,
                    cs.published_tx_index,
                    cs.published_log_index,
                    cs.reorg_correction
                FROM channel c
                JOIN channel_state cs ON cs.channel_id = c.id
                WHERE cs.id = (
                    SELECT s2.id FROM channel_state s2
                    WHERE s2.channel_id = c.id
                    ORDER BY s2.published_block DESC, s2.published_tx_index DESC, s2.published_log_index DESC
                    LIMIT 1
                )"
            ))
            .await?;

        manager
            .get_connection()
            .execute_unprepared(&format!(
                "{create_view} account_current AS
                SELECT
                    acs.id,
                    a.id AS account_id,
                    a.chain_key,
                    a.packet_key,
                    acs.safe_address,
                    acs.published_block,
                    acs.published_tx_index,
                    acs.published_log_index
                FROM account a
                JOIN account_state acs ON acs.account_id = a.id
                WHERE acs.id = (
                    SELECT s2.id FROM account_state s2
                    WHERE s2.account_id = a.id
                    ORDER BY s2.published_block DESC, s2.published_tx_index DESC, s2.published_log_index DESC
                    LIMIT 1
                )"
            ))
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let backend = manager.get_database_backend();

        if backend != sea_orm::DatabaseBackend::Postgres {
            manager
                .get_connection()
                .execute_unprepared("DROP VIEW IF EXISTS channel_current")
                .await?;
            manager
                .get_connection()
                .execute_unprepared("DROP VIEW IF EXISTS account_current")
                .await?;
        }

        let create_view = if backend == sea_orm::DatabaseBackend::Postgres {
            "CREATE OR REPLACE VIEW"
        } else {
            "CREATE VIEW IF NOT EXISTS"
        };

        // Restore the original window-function definitions from m001.
        manager
            .get_connection()
            .execute_unprepared(&format!(
                "{create_view} channel_current AS
                SELECT
                    s.id,
                    c.id AS channel_id,
                    c.concrete_channel_id,
                    c.source,
                    c.destination,
                    s.balance,
                    s.status,
                    s.epoch,
                    s.ticket_index,
                    s.closure_time,
                    s.corrupted_state,
                    s.published_block,
                    s.published_tx_index,
                    s.published_log_index,
                    s.reorg_correction
                FROM channel c
                JOIN (
                    SELECT cs.*, ROW_NUMBER() OVER (
                        PARTITION BY cs.channel_id
                        ORDER BY cs.published_block DESC, cs.published_tx_index DESC, cs.published_log_index DESC
                    ) AS rn
                    FROM channel_state cs
                ) s ON s.channel_id = c.id AND s.rn = 1"
            ))
            .await?;

        manager
            .get_connection()
            .execute_unprepared(&format!(
                "{create_view} account_current AS
                SELECT
                    s.id,
                    a.id AS account_id,
                    a.chain_key,
                    a.packet_key,
                    s.safe_address,
                    s.published_block,
                    s.published_tx_index,
                    s.published_log_index
                FROM account a
                JOIN (
                    SELECT acs.*, ROW_NUMBER() OVER (
                        PARTITION BY acs.account_id
                        ORDER BY acs.published_block DESC, acs.published_tx_index DESC, acs.published_log_index DESC
                    ) AS rn
                    FROM account_state acs
                ) s ON s.account_id = a.id AND s.rn = 1"
            ))
            .await?;

        Ok(())
    }
}
