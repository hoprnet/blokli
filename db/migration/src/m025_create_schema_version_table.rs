use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Create schema_version table to track database schema version
        // This enables automatic data clearing when schema version increases
        manager
            .create_table(
                Table::create()
                    .table(SchemaVersion::Table)
                    .col(ColumnDef::new(SchemaVersion::Id).big_integer().not_null().primary_key())
                    .col(ColumnDef::new(SchemaVersion::Version).big_integer().not_null())
                    .col(
                        ColumnDef::new(SchemaVersion::UpdatedAt)
                            .timestamp()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .to_owned(),
            )
            .await?;

        // Insert initial version = 1
        // The migration to BIGINT (m023, m024) will bump this to version 2 in code
        manager
            .exec_stmt(
                Query::insert()
                    .into_table(SchemaVersion::Table)
                    .columns([SchemaVersion::Id, SchemaVersion::Version])
                    .values_panic([1.into(), 1.into()])
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Drop schema_version table
        manager
            .drop_table(Table::drop().table(SchemaVersion::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum SchemaVersion {
    Table,
    Id,
    Version,
    UpdatedAt,
}
