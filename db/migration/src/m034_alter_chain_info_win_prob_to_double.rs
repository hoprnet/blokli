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
                // SQLite stores all floating point values as 64-bit IEEE 754 REAL internally,
                // regardless of the declared column type (FLOAT or DOUBLE). No schema change needed.
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
                // SQLite: No change needed (see up() comments)
            }
            _ => {}
        }

        Ok(())
    }
}
