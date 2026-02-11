use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Drop existing views so we can recreate them with additional columns
        manager
            .get_connection()
            .execute_unprepared("DROP VIEW IF EXISTS channel_current")
            .await?;

        manager
            .get_connection()
            .execute_unprepared("DROP VIEW IF EXISTS account_current")
            .await?;

        // Recreate channel_current view with s.id and s.reorg_correction columns
        let create_channel_current_view = r#"
            CREATE VIEW channel_current AS
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
                SELECT
                    cs.*,
                    ROW_NUMBER() OVER (
                        PARTITION BY cs.channel_id
                        ORDER BY cs.published_block DESC, cs.published_tx_index DESC, cs.published_log_index DESC
                    ) AS rn
                FROM channel_state cs
            ) s ON s.channel_id = c.id AND s.rn = 1
        "#;

        manager
            .get_connection()
            .execute_unprepared(create_channel_current_view)
            .await?;

        // Recreate account_current view with s.id column
        let create_account_current_view = r#"
            CREATE VIEW account_current AS
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
                SELECT
                    acs.*,
                    ROW_NUMBER() OVER (
                        PARTITION BY acs.account_id
                        ORDER BY acs.published_block DESC, acs.published_tx_index DESC, acs.published_log_index DESC
                    ) AS rn
                FROM account_state acs
            ) s ON s.account_id = a.id AND s.rn = 1
        "#;

        manager
            .get_connection()
            .execute_unprepared(create_account_current_view)
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Drop the updated views
        manager
            .get_connection()
            .execute_unprepared("DROP VIEW IF EXISTS channel_current")
            .await?;

        manager
            .get_connection()
            .execute_unprepared("DROP VIEW IF EXISTS account_current")
            .await?;

        // Recreate the original views without the extra columns
        let create_channel_current_view = r#"
            CREATE VIEW channel_current AS
            SELECT
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
                s.published_log_index
            FROM channel c
            JOIN (
                SELECT
                    cs.*,
                    ROW_NUMBER() OVER (
                        PARTITION BY cs.channel_id
                        ORDER BY cs.published_block DESC, cs.published_tx_index DESC, cs.published_log_index DESC
                    ) AS rn
                FROM channel_state cs
            ) s ON s.channel_id = c.id AND s.rn = 1
        "#;

        manager
            .get_connection()
            .execute_unprepared(create_channel_current_view)
            .await?;

        let create_account_current_view = r#"
            CREATE VIEW account_current AS
            SELECT
                a.id AS account_id,
                a.chain_key,
                a.packet_key,
                s.safe_address,
                s.published_block,
                s.published_tx_index,
                s.published_log_index
            FROM account a
            JOIN (
                SELECT
                    acs.*,
                    ROW_NUMBER() OVER (
                        PARTITION BY acs.account_id
                        ORDER BY acs.published_block DESC, acs.published_tx_index DESC, acs.published_log_index DESC
                    ) AS rn
                FROM account_state acs
            ) s ON s.account_id = a.id AND s.rn = 1
        "#;

        manager
            .get_connection()
            .execute_unprepared(create_account_current_view)
            .await?;

        Ok(())
    }
}
