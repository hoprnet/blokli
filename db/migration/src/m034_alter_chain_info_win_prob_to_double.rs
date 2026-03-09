use hopr_types::internal::prelude::DEFAULT_MINIMUM_INCOMING_TICKET_WIN_PROB;
use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        match manager.get_database_backend() {
            sea_orm::DatabaseBackend::Postgres => {
                manager
                    .get_connection()
                    .execute_unprepared(
                        "ALTER TABLE chain_info ALTER COLUMN min_incoming_ticket_win_prob TYPE DOUBLE PRECISION USING \
                         min_incoming_ticket_win_prob::double precision",
                    )
                    .await?;
            }
            sea_orm::DatabaseBackend::Sqlite => {
                manager
                    .get_connection()
                    .execute_unprepared(
                        "CREATE TABLE chain_info_min_incoming_ticket_win_prob_backup (id INTEGER PRIMARY KEY, \
                         min_incoming_ticket_win_prob FLOAT)",
                    )
                    .await?;
                manager
                    .get_connection()
                    .execute_unprepared(
                        "INSERT INTO chain_info_min_incoming_ticket_win_prob_backup (id, \
                         min_incoming_ticket_win_prob) SELECT id, min_incoming_ticket_win_prob FROM chain_info",
                    )
                    .await?;
                manager
                    .get_connection()
                    .execute_unprepared("ALTER TABLE chain_info DROP COLUMN min_incoming_ticket_win_prob")
                    .await?;
                manager
                    .get_connection()
                    .execute_unprepared(&format!(
                        "ALTER TABLE chain_info ADD COLUMN min_incoming_ticket_win_prob DOUBLE NOT NULL DEFAULT {}",
                        DEFAULT_MINIMUM_INCOMING_TICKET_WIN_PROB
                    ))
                    .await?;
                manager
                    .get_connection()
                    .execute_unprepared(
                        "UPDATE chain_info SET min_incoming_ticket_win_prob = (SELECT \
                         CAST(min_incoming_ticket_win_prob AS REAL) FROM \
                         chain_info_min_incoming_ticket_win_prob_backup WHERE \
                         chain_info_min_incoming_ticket_win_prob_backup.id = chain_info.id)",
                    )
                    .await?;
                manager
                    .get_connection()
                    .execute_unprepared("DROP TABLE chain_info_min_incoming_ticket_win_prob_backup")
                    .await?;
            }
            _ => {}
        }

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        match manager.get_database_backend() {
            sea_orm::DatabaseBackend::Postgres => {
                manager
                    .get_connection()
                    .execute_unprepared(
                        "ALTER TABLE chain_info ALTER COLUMN min_incoming_ticket_win_prob TYPE REAL USING \
                         min_incoming_ticket_win_prob::real",
                    )
                    .await?;
            }
            sea_orm::DatabaseBackend::Sqlite => {
                manager
                    .get_connection()
                    .execute_unprepared(
                        "CREATE TABLE chain_info_min_incoming_ticket_win_prob_backup (id INTEGER PRIMARY KEY, \
                         min_incoming_ticket_win_prob DOUBLE)",
                    )
                    .await?;
                manager
                    .get_connection()
                    .execute_unprepared(
                        "INSERT INTO chain_info_min_incoming_ticket_win_prob_backup (id, \
                         min_incoming_ticket_win_prob) SELECT id, min_incoming_ticket_win_prob FROM chain_info",
                    )
                    .await?;
                manager
                    .get_connection()
                    .execute_unprepared("ALTER TABLE chain_info DROP COLUMN min_incoming_ticket_win_prob")
                    .await?;
                manager
                    .get_connection()
                    .execute_unprepared(&format!(
                        "ALTER TABLE chain_info ADD COLUMN min_incoming_ticket_win_prob FLOAT NOT NULL DEFAULT {}",
                        DEFAULT_MINIMUM_INCOMING_TICKET_WIN_PROB
                    ))
                    .await?;
                manager
                    .get_connection()
                    .execute_unprepared(
                        "UPDATE chain_info SET min_incoming_ticket_win_prob = (SELECT \
                         CAST(min_incoming_ticket_win_prob AS REAL) FROM \
                         chain_info_min_incoming_ticket_win_prob_backup WHERE \
                         chain_info_min_incoming_ticket_win_prob_backup.id = chain_info.id)",
                    )
                    .await?;
                manager
                    .get_connection()
                    .execute_unprepared("DROP TABLE chain_info_min_incoming_ticket_win_prob_backup")
                    .await?;
            }
            _ => {}
        }

        Ok(())
    }
}
