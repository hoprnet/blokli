use sea_orm_migration::prelude::*;

/// Allows a node to move from one Safe to another.
///
/// The database no longer rejects an insertion before the indexer removes the
/// node's previous binding. Event coordinates and `(safe_address, node_address)`
/// remain unique, so event replay is still idempotent.
#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        if manager.get_database_backend() == sea_orm::DatabaseBackend::Postgres {
            manager
                .get_connection()
                .execute_unprepared(
                    "ALTER TABLE hopr_node_safe_registration DROP CONSTRAINT IF EXISTS \
                     hopr_node_safe_registration_node_address_key",
                )
                .await?;
        }

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        if manager.get_database_backend() == sea_orm::DatabaseBackend::Postgres {
            manager
                .get_connection()
                .execute_unprepared(
                    "ALTER TABLE hopr_node_safe_registration ADD CONSTRAINT \
                     hopr_node_safe_registration_node_address_key UNIQUE (node_address)",
                )
                .await?;
        }

        Ok(())
    }
}
