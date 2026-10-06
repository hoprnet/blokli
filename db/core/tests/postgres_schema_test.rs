//! Checks that every SeaORM entity matches the schema the migrations build on PostgreSQL.
//!
//! SQLite stores every integer as 64-bit and is lenient about column types, so an entity field
//! declared as `i64` over an `INTEGER` (`INT4`) column passes every SQLite test and only fails at
//! runtime on PostgreSQL, when a row is decoded. This test catches that class of drift before a
//! release by comparing the entity column definitions against `information_schema`.
//!
//! It needs a PostgreSQL server and is therefore ignored by default. Run it with
//! `just test-postgres-schema`, or point `BLOKLI_TEST_POSTGRES_URL` at a server whose user may
//! create databases and run `cargo test -p blokli-db --test postgres_schema_test -- --ignored`.
//! A scratch database is created and dropped, so the database named in the URL is left untouched.

use std::{
    collections::{BTreeMap, BTreeSet},
    env, process,
};

use anyhow::{Context, Result, bail};
use blokli_db_entity::{
    account, account_state, announcement, chain_info, channel, channel_state, curvy_committed_note,
    curvy_committed_nullifier, curvy_pending_note, curvy_shard_root, curvy_sync_checkpoint, hopr_balance,
    hopr_node_safe_registration, hopr_safe_contract, hopr_safe_contract_state, hopr_safe_event,
    hopr_safe_execution_event, hopr_safe_owner_change_event, hopr_safe_owner_state, hopr_safe_redeemed_stats,
    hopr_safe_setup_event, hopr_safe_setup_owner, hopr_safe_threshold_change_event, hopr_safe_threshold_state, log,
    log_status, log_topic_info, native_balance, schema_version, service_entry, service_entry_state,
    service_registry_config, service_type, service_type_state,
    views::{
        account_current, channel_current, safe_contract_current, safe_owner_current, safe_threshold_current,
        service_entry_current, service_type_current,
    },
};
use migration::{Migrator, MigratorTrait};
use sea_orm::{
    ColumnTrait, ColumnType, ConnectionTrait, Database, DatabaseConnection, DbBackend, EntityTrait, IdenStatic,
    Iterable, Statement,
};
use url::Url;

const POSTGRES_URL_VAR: &str = "BLOKLI_TEST_POSTGRES_URL";

/// One column of an entity, as SeaORM will decode it.
struct EntityColumn {
    name: String,
    column_type: ColumnType,
    nullable: bool,
}

/// One column of a table or view, as PostgreSQL reports it.
struct DbColumn {
    udt_name: String,
    nullable: bool,
}

struct DbRelation {
    is_view: bool,
    columns: BTreeMap<String, DbColumn>,
}

fn entity_columns<E: EntityTrait>() -> (String, Vec<EntityColumn>) {
    let columns = E::Column::iter()
        .map(|column| {
            let def = column.def();
            EntityColumn {
                name: column.as_str().to_string(),
                column_type: def.get_column_type().clone(),
                nullable: def.is_null(),
            }
        })
        .collect();
    (E::default().table_name().to_string(), columns)
}

/// Every entity the application decodes rows into. A table or view without an entry here, or an
/// entry without a table or view, fails the test, so this list cannot silently fall behind.
fn all_entities() -> Vec<(String, Vec<EntityColumn>)> {
    vec![
        entity_columns::<account::Entity>(),
        entity_columns::<account_state::Entity>(),
        entity_columns::<announcement::Entity>(),
        entity_columns::<chain_info::Entity>(),
        entity_columns::<channel::Entity>(),
        entity_columns::<channel_state::Entity>(),
        entity_columns::<curvy_committed_note::Entity>(),
        entity_columns::<curvy_committed_nullifier::Entity>(),
        entity_columns::<curvy_pending_note::Entity>(),
        entity_columns::<curvy_shard_root::Entity>(),
        entity_columns::<curvy_sync_checkpoint::Entity>(),
        entity_columns::<hopr_balance::Entity>(),
        entity_columns::<hopr_node_safe_registration::Entity>(),
        entity_columns::<hopr_safe_contract::Entity>(),
        entity_columns::<hopr_safe_contract_state::Entity>(),
        entity_columns::<hopr_safe_event::Entity>(),
        entity_columns::<hopr_safe_execution_event::Entity>(),
        entity_columns::<hopr_safe_owner_change_event::Entity>(),
        entity_columns::<hopr_safe_owner_state::Entity>(),
        entity_columns::<hopr_safe_redeemed_stats::Entity>(),
        entity_columns::<hopr_safe_setup_event::Entity>(),
        entity_columns::<hopr_safe_setup_owner::Entity>(),
        entity_columns::<hopr_safe_threshold_change_event::Entity>(),
        entity_columns::<hopr_safe_threshold_state::Entity>(),
        entity_columns::<log::Entity>(),
        entity_columns::<log_status::Entity>(),
        entity_columns::<log_topic_info::Entity>(),
        entity_columns::<native_balance::Entity>(),
        entity_columns::<schema_version::Entity>(),
        entity_columns::<service_entry::Entity>(),
        entity_columns::<service_entry_state::Entity>(),
        entity_columns::<service_registry_config::Entity>(),
        entity_columns::<service_type::Entity>(),
        entity_columns::<service_type_state::Entity>(),
        entity_columns::<account_current::Entity>(),
        entity_columns::<channel_current::Entity>(),
        entity_columns::<safe_contract_current::Entity>(),
        entity_columns::<safe_owner_current::Entity>(),
        entity_columns::<safe_threshold_current::Entity>(),
        entity_columns::<service_entry_current::Entity>(),
        entity_columns::<service_type_current::Entity>(),
    ]
}

/// The PostgreSQL types (`udt_name`) that sqlx decodes into the Rust type behind `column_type`.
fn compatible_udt_names(column_type: &ColumnType) -> Option<&'static [&'static str]> {
    let names: &'static [&'static str] = match column_type {
        ColumnType::BigInteger | ColumnType::BigUnsigned => &["int8"],
        ColumnType::Integer | ColumnType::Unsigned => &["int4"],
        ColumnType::SmallInteger | ColumnType::SmallUnsigned | ColumnType::TinyInteger | ColumnType::TinyUnsigned => {
            &["int2"]
        }
        ColumnType::Boolean => &["bool"],
        ColumnType::Char(_) | ColumnType::String(_) | ColumnType::Text => &["text", "varchar", "bpchar"],
        ColumnType::Binary(_) | ColumnType::VarBinary(_) | ColumnType::Blob => &["bytea"],
        ColumnType::Double => &["float8"],
        ColumnType::Float => &["float4"],
        ColumnType::Decimal(_) | ColumnType::Money(_) => &["numeric"],
        ColumnType::TimestampWithTimeZone => &["timestamptz"],
        ColumnType::DateTime | ColumnType::Timestamp => &["timestamp"],
        ColumnType::Date => &["date"],
        ColumnType::Time => &["time"],
        ColumnType::Json => &["json"],
        ColumnType::JsonBinary => &["jsonb"],
        ColumnType::Uuid => &["uuid"],
        _ => return None,
    };
    Some(names)
}

async fn load_db_relations(db: &DatabaseConnection) -> Result<BTreeMap<String, DbRelation>> {
    let rows = db
        .query_all_raw(Statement::from_string(
            DbBackend::Postgres,
            "SELECT c.table_name, c.column_name, c.udt_name, c.is_nullable, t.table_type FROM \
             information_schema.columns c JOIN information_schema.tables t ON t.table_schema = c.table_schema AND \
             t.table_name = c.table_name WHERE c.table_schema = current_schema() AND c.table_name <> \
             'seaql_migrations'"
                .to_string(),
        ))
        .await?;

    let mut relations: BTreeMap<String, DbRelation> = BTreeMap::new();
    for row in rows {
        let table: String = row.try_get("", "table_name")?;
        let column: String = row.try_get("", "column_name")?;
        let udt_name: String = row.try_get("", "udt_name")?;
        let is_nullable: String = row.try_get("", "is_nullable")?;
        let table_type: String = row.try_get("", "table_type")?;

        relations
            .entry(table)
            .or_insert_with(|| DbRelation {
                is_view: table_type == "VIEW",
                columns: BTreeMap::new(),
            })
            .columns
            .insert(
                column,
                DbColumn {
                    udt_name,
                    nullable: is_nullable == "YES",
                },
            );
    }
    Ok(relations)
}

/// Lists every way the entities disagree with the migrated schema.
fn schema_mismatches(
    entities: &[(String, Vec<EntityColumn>)],
    relations: &BTreeMap<String, DbRelation>,
) -> Vec<String> {
    let mut mismatches = Vec::new();

    for (table, columns) in entities {
        let Some(relation) = relations.get(table) else {
            mismatches.push(format!(
                "{table}: entity exists but the migrations create no such table or view"
            ));
            continue;
        };

        let entity_column_names: BTreeSet<&str> = columns.iter().map(|c| c.name.as_str()).collect();
        for db_column in relation.columns.keys() {
            if !entity_column_names.contains(db_column.as_str()) {
                mismatches.push(format!(
                    "{table}.{db_column}: column exists in the database but not in the entity"
                ));
            }
        }

        for column in columns {
            let name = &column.name;
            let Some(db_column) = relation.columns.get(name) else {
                mismatches.push(format!("{table}.{name}: entity column missing from the database"));
                continue;
            };

            match compatible_udt_names(&column.column_type) {
                Some(names) if names.contains(&db_column.udt_name.as_str()) => {}
                Some(names) => mismatches.push(format!(
                    "{table}.{name}: entity {:?} expects {names:?}, database has {}",
                    column.column_type, db_column.udt_name
                )),
                None => mismatches.push(format!(
                    "{table}.{name}: entity type {:?} is not covered by this test, extend `compatible_udt_names`",
                    column.column_type
                )),
            }

            // PostgreSQL reports every view column as nullable, so only tables are checked.
            if !relation.is_view && db_column.nullable && !column.nullable {
                mismatches.push(format!(
                    "{table}.{name}: database column is nullable but the entity field is not an Option"
                ));
            }
        }
    }

    let entity_tables: BTreeSet<&str> = entities.iter().map(|(table, _)| table.as_str()).collect();
    for table in relations.keys() {
        if !entity_tables.contains(table.as_str()) {
            mismatches.push(format!("{table}: table or view has no entity listed in `all_entities`"));
        }
    }

    mismatches
}

async fn check_schema(url: &Url) -> Result<Vec<String>> {
    let db = Database::connect(url.as_str()).await?;
    Migrator::up(&db, None)
        .await
        .context("migrations failed on PostgreSQL")?;
    let relations = load_db_relations(&db).await?;
    db.close().await?;

    Ok(schema_mismatches(&all_entities(), &relations))
}

#[tokio::test]
#[ignore = "needs PostgreSQL, run with `just test-postgres-schema`"]
async fn entities_match_postgres_schema() -> Result<()> {
    let admin_url: Url = env::var(POSTGRES_URL_VAR)
        .with_context(|| format!("{POSTGRES_URL_VAR} must point at a PostgreSQL server"))?
        .parse()?;

    let scratch_db = format!("blokli_schema_check_{}", process::id());
    let mut scratch_url = admin_url.clone();
    scratch_url.set_path(&scratch_db);

    let admin = Database::connect(admin_url.as_str()).await?;
    admin
        .execute_unprepared(&format!("CREATE DATABASE \"{scratch_db}\""))
        .await?;

    let result = check_schema(&scratch_url).await;

    admin
        .execute_unprepared(&format!("DROP DATABASE IF EXISTS \"{scratch_db}\""))
        .await?;

    let mismatches = result?;
    if !mismatches.is_empty() {
        bail!(
            "{} entity/schema mismatch(es) on PostgreSQL:\n  {}",
            mismatches.len(),
            mismatches.join("\n  ")
        );
    }
    Ok(())
}
