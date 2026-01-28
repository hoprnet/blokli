use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Remove the PostgreSQL trigger and function that was used for database pub/sub
        // This is no longer needed since we migrated to the IndexerState event bus
        if manager.get_database_backend() == sea_orm::DatabaseBackend::Postgres {
            manager
                .get_connection()
                .execute_unprepared(
                    r#"
                    DROP TRIGGER IF EXISTS trigger_notify_ticket_params ON chain_info;
                    DROP FUNCTION IF EXISTS notify_ticket_params_changed();
                    "#,
                )
                .await?;
        }
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Recreate the trigger if we need to roll back
        if manager.get_database_backend() == sea_orm::DatabaseBackend::Postgres {
            manager
                .get_connection()
                .execute_unprepared(
                    r#"
                    CREATE OR REPLACE FUNCTION notify_ticket_params_changed()
                    RETURNS TRIGGER AS $$
                    BEGIN
                        IF (OLD.ticket_price IS DISTINCT FROM NEW.ticket_price) OR
                           (OLD.min_incoming_ticket_win_prob IS DISTINCT FROM NEW.min_incoming_ticket_win_prob) THEN
                            PERFORM pg_notify('ticket_params_updated', '');
                        END IF;
                        RETURN NEW;
                    END;
                    $$ LANGUAGE plpgsql;

                    CREATE TRIGGER trigger_notify_ticket_params
                    AFTER UPDATE ON chain_info
                    FOR EACH ROW
                    EXECUTE FUNCTION notify_ticket_params_changed();
                    "#,
                )
                .await?;
        }
        Ok(())
    }
}
