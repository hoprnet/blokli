use sea_orm::ConnectionTrait;
use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();

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
