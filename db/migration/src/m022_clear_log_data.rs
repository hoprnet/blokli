use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Clear log data from tables, respecting foreign key relationships
        // Delete in order of dependencies

        // Clear log_status (depends on log)
        manager
            .exec_stmt(Query::delete().from_table(LogStatus::Table).to_owned())
            .await?;

        // Clear log and log_topic_info (no dependencies)
        manager
            .exec_stmt(Query::delete().from_table(Log::Table).to_owned())
            .await?;

        manager
            .exec_stmt(Query::delete().from_table(LogTopicInfo::Table).to_owned())
            .await
    }

    async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        // Down migration is a no-op for data reset
        // Schema is preserved, so no action needed to reverse
        Ok(())
    }
}

#[derive(DeriveIden)]
enum Log {
    Table,
}

#[derive(DeriveIden)]
enum LogStatus {
    Table,
}

#[derive(DeriveIden)]
enum LogTopicInfo {
    Table,
}
