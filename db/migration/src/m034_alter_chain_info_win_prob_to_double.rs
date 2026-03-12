use sea_orm::ConnectionTrait;
use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

/// SQLite table recreation SQL for `up`: changes min_incoming_ticket_win_prob from float to double.
///
/// SQLite does not support ALTER COLUMN, and DROP COLUMN + ADD COLUMN fails silently
/// due to interactions between sqlx's foreign_keys pragma and statement caching.
/// The workaround is to recreate the table with the desired schema.
const SQLITE_UP: &str = "
ALTER TABLE chain_info RENAME TO chain_info_old;
CREATE TABLE chain_info (
    id integer NOT NULL PRIMARY KEY AUTOINCREMENT,
    last_indexed_block integer NOT NULL DEFAULT 0,
    ticket_price blob(12),
    channels_dst blob(32),
    ledger_dst blob(32),
    safe_registry_dst blob(32),
    min_incoming_ticket_win_prob double NOT NULL DEFAULT 1.0,
    channel_closure_grace_period integer,
    last_indexed_tx_index integer NOT NULL DEFAULT 0,
    last_indexed_log_index integer NOT NULL DEFAULT 0,
    key_binding_fee blob(12)
);
INSERT INTO chain_info SELECT
    id, last_indexed_block, ticket_price, channels_dst, ledger_dst, safe_registry_dst,
    min_incoming_ticket_win_prob, channel_closure_grace_period,
    last_indexed_tx_index, last_indexed_log_index, key_binding_fee
FROM chain_info_old;
DROP TABLE chain_info_old;
";

/// SQLite table recreation SQL for `down`: changes min_incoming_ticket_win_prob from double to float.
const SQLITE_DOWN: &str = "
ALTER TABLE chain_info RENAME TO chain_info_old;
CREATE TABLE chain_info (
    id integer NOT NULL PRIMARY KEY AUTOINCREMENT,
    last_indexed_block integer NOT NULL DEFAULT 0,
    ticket_price blob(12),
    channels_dst blob(32),
    ledger_dst blob(32),
    safe_registry_dst blob(32),
    min_incoming_ticket_win_prob float NOT NULL DEFAULT 1.0,
    channel_closure_grace_period integer,
    last_indexed_tx_index integer NOT NULL DEFAULT 0,
    last_indexed_log_index integer NOT NULL DEFAULT 0,
    key_binding_fee blob(12)
);
INSERT INTO chain_info SELECT
    id, last_indexed_block, ticket_price, channels_dst, ledger_dst, safe_registry_dst,
    min_incoming_ticket_win_prob, channel_closure_grace_period,
    last_indexed_tx_index, last_indexed_log_index, key_binding_fee
FROM chain_info_old;
DROP TABLE chain_info_old;
";

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();

        if matches!(db.get_database_backend(), sea_orm::DatabaseBackend::Sqlite) {
            // Temporarily disable foreign keys so DROP TABLE works reliably.
            // sqlx sets PRAGMA foreign_keys = ON which can interfere with table operations.
            db.execute_unprepared("PRAGMA foreign_keys = OFF").await?;
            for statement in SQLITE_UP.trim().split(';').filter(|s| !s.trim().is_empty()) {
                db.execute_unprepared(statement.trim()).await?;
            }
            db.execute_unprepared("PRAGMA foreign_keys = ON").await?;
            return Ok(());
        }

        // PostgreSQL: DROP COLUMN + ADD COLUMN with value preservation.

        // 1. Read current values before dropping the column
        let select = Query::select()
            .columns([ChainInfo::Id, ChainInfo::MinIncomingTicketWinProb])
            .from(ChainInfo::Table)
            .to_owned();

        let rows = db.query_all(&select).await?;

        let mut values: Vec<(i64, f64)> = Vec::new();
        for row in rows {
            let id: i64 = row.try_get("", "id")?;
            let win_prob: f64 = row.try_get("", "min_incoming_ticket_win_prob")?;
            values.push((id, win_prob));
        }

        // 2. Drop the old float column
        manager
            .alter_table(
                Table::alter()
                    .table(ChainInfo::Table)
                    .drop_column(ChainInfo::MinIncomingTicketWinProb)
                    .to_owned(),
            )
            .await?;

        // 3. Add it back as double
        manager
            .alter_table(
                Table::alter()
                    .table(ChainInfo::Table)
                    .add_column(
                        ColumnDef::new(ChainInfo::MinIncomingTicketWinProb)
                            .double()
                            .not_null()
                            .default(1.0),
                    )
                    .to_owned(),
            )
            .await?;

        // 4. Restore values
        for (id, win_prob) in values {
            manager
                .exec_stmt(
                    Query::update()
                        .table(ChainInfo::Table)
                        .value(ChainInfo::MinIncomingTicketWinProb, win_prob)
                        .and_where(Expr::col(ChainInfo::Id).eq(id))
                        .to_owned(),
                )
                .await?;
        }

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();

        if matches!(db.get_database_backend(), sea_orm::DatabaseBackend::Sqlite) {
            db.execute_unprepared("PRAGMA foreign_keys = OFF").await?;
            for statement in SQLITE_DOWN.trim().split(';').filter(|s| !s.trim().is_empty()) {
                db.execute_unprepared(statement.trim()).await?;
            }
            db.execute_unprepared("PRAGMA foreign_keys = ON").await?;
            return Ok(());
        }

        // PostgreSQL: DROP COLUMN + ADD COLUMN with value preservation.

        // 1. Read current values before dropping the column
        let select = Query::select()
            .columns([ChainInfo::Id, ChainInfo::MinIncomingTicketWinProb])
            .from(ChainInfo::Table)
            .to_owned();

        let rows = db.query_all(&select).await?;

        let mut values: Vec<(i64, f64)> = Vec::new();
        for row in rows {
            let id: i64 = row.try_get("", "id")?;
            let win_prob: f64 = row.try_get("", "min_incoming_ticket_win_prob")?;
            values.push((id, win_prob));
        }

        // 2. Drop the double column
        manager
            .alter_table(
                Table::alter()
                    .table(ChainInfo::Table)
                    .drop_column(ChainInfo::MinIncomingTicketWinProb)
                    .to_owned(),
            )
            .await?;

        // 3. Add it back as float
        manager
            .alter_table(
                Table::alter()
                    .table(ChainInfo::Table)
                    .add_column(
                        ColumnDef::new(ChainInfo::MinIncomingTicketWinProb)
                            .float()
                            .not_null()
                            .default(1.0),
                    )
                    .to_owned(),
            )
            .await?;

        // 4. Restore values (precision may be reduced from f64 to f32)
        for (id, win_prob) in values {
            manager
                .exec_stmt(
                    Query::update()
                        .table(ChainInfo::Table)
                        .value(ChainInfo::MinIncomingTicketWinProb, win_prob)
                        .and_where(Expr::col(ChainInfo::Id).eq(id))
                        .to_owned(),
                )
                .await?;
        }

        Ok(())
    }
}

#[derive(DeriveIden)]
#[allow(dead_code)]
enum ChainInfo {
    Table,
    Id,
    LastIndexedBlock,
    TicketPrice,
    ChannelsDST,
    LedgerDST,
    SafeRegistryDST,
    MinIncomingTicketWinProb,
    ChannelClosureGracePeriod,
}
