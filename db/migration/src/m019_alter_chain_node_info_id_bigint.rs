use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Fix type mismatch: change id columns from INTEGER (INT4) to BIGINT (INT8)
        // to match SeaORM entity models which expect i64
        //
        // SQLite uses 64-bit INTEGER by default, so no change needed
        // PostgreSQL uses 32-bit INTEGER, needs explicit BIGINT

        match manager.get_database_backend() {
            sea_orm::DatabaseBackend::Postgres => {
                manager
                    .get_connection()
                    .execute_unprepared("ALTER TABLE chain_info ALTER COLUMN id TYPE BIGINT")
                    .await?;

                manager
                    .get_connection()
                    .execute_unprepared("ALTER TABLE node_info ALTER COLUMN id TYPE BIGINT")
                    .await?;
            }
            sea_orm::DatabaseBackend::Sqlite => {
                // SQLite: INTEGER is already 64-bit, no change needed
            }
            _ => {
                // Unknown database backend, skip migration
            }
        }

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Downgrade from BIGINT back to INTEGER
        // This is safe if all IDs fit in INT4 range

        match manager.get_database_backend() {
            sea_orm::DatabaseBackend::Postgres => {
                manager
                    .get_connection()
                    .execute_unprepared("ALTER TABLE node_info ALTER COLUMN id TYPE INTEGER")
                    .await?;

                manager
                    .get_connection()
                    .execute_unprepared("ALTER TABLE chain_info ALTER COLUMN id TYPE INTEGER")
                    .await?;
            }
            sea_orm::DatabaseBackend::Sqlite => {
                // SQLite: No change needed
            }
            _ => {
                // Unknown database backend, skip migration
            }
        }

        Ok(())
    }
}
