use sea_orm_migration::prelude::*;

/// Aligns PostgreSQL column types with the types the SeaORM entities decode them into.
///
/// Earlier migrations created some columns with a narrower type than their entity field:
/// - the Curvy `INTEGER` (`INT4`) columns of `m007_curvy_note_tree`, which the entities model as `i64`;
/// - `schema_version.updated_at`, a `TIMESTAMP` that the entity models as a UTC `TIMESTAMPTZ`.
///
/// SQLite is lenient about both (every integer is 64-bit, timestamps are untyped), so the mismatch only
/// surfaces on PostgreSQL, when a row is decoded. SQLite also cannot alter a column type, so this
/// migration is PostgreSQL-only. `db/core/tests/postgres_schema_test.rs` guards against new drift.
#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        if manager.get_database_backend() != sea_orm::DatabaseBackend::Postgres {
            return Ok(());
        }

        for (table, columns) in curvy_integer_columns() {
            alter_columns(manager, table, columns, |col| col.big_integer().not_null()).await?;
        }
        alter_columns(manager, schema_version_table(), schema_version_columns(), |col| {
            col.timestamp_with_time_zone().not_null()
        })
        .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        if manager.get_database_backend() != sea_orm::DatabaseBackend::Postgres {
            return Ok(());
        }

        for (table, columns) in curvy_integer_columns() {
            alter_columns(manager, table, columns, |col| col.integer().not_null()).await?;
        }
        alter_columns(manager, schema_version_table(), schema_version_columns(), |col| {
            col.timestamp().not_null()
        })
        .await
    }
}

/// The `INTEGER` columns of `m007_curvy_note_tree`, grouped by table.
fn curvy_integer_columns() -> Vec<(Alias, Vec<Alias>)> {
    [
        ("curvy_pending_note", &["view_tag", "event_item_index"][..]),
        ("curvy_committed_note", &["event_item_index"][..]),
        ("curvy_committed_nullifier", &["event_item_index"][..]),
        ("curvy_shard_root", &["tree_version", "shard_height"][..]),
        (
            "curvy_sync_checkpoint",
            &["tree_version", "tree_depth", "shard_height"][..],
        ),
    ]
    .into_iter()
    .map(|(table, columns)| (Alias::new(table), columns.iter().map(|c| Alias::new(*c)).collect()))
    .collect()
}

fn schema_version_table() -> Alias {
    Alias::new("schema_version")
}

fn schema_version_columns() -> Vec<Alias> {
    vec![Alias::new("updated_at")]
}

async fn alter_columns(
    manager: &SchemaManager<'_>,
    table: Alias,
    columns: Vec<Alias>,
    column_type: impl Fn(&mut ColumnDef) -> &mut ColumnDef,
) -> Result<(), DbErr> {
    let mut statement = Table::alter();
    statement.table(table);
    for column in columns {
        statement.modify_column(column_type(&mut ColumnDef::new(column)));
    }

    manager.alter_table(statement).await
}
