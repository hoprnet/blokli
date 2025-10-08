//! Creates a build specification for the ORM codegen.

use std::{env, path::Path};

use anyhow::Context;
use clap::Parser;
use sea_orm::{ConnectOptions, Database};
use sea_orm_cli::{Cli, Commands, run_generate_command};

async fn execute_sea_orm_cli_command<I, T>(itr: I) -> anyhow::Result<()>
where
    I: IntoIterator<Item = T>,
    T: Into<std::ffi::OsString> + Clone,
{
    let cli = Cli::try_parse_from(itr).context("should be able to parse a sea-orm-cli command")?;

    match cli.command {
        Commands::Generate { command } => run_generate_command(command, true)
            .await
            .map_err(|e| anyhow::anyhow!(e.to_string())),
        Commands::Migrate {
            database_schema,
            database_url,
            command,
            ..
        } => {
            let connect_options: ConnectOptions = ConnectOptions::new(
                database_url.unwrap_or("postgresql://bloklid:bloklid@localhost:5432/bloklid_codegen".into()),
            )
            .set_schema_search_path(database_schema.unwrap_or_else(|| "public".to_owned()))
            .to_owned();
            let db = &Database::connect(connect_options).await?;

            sea_orm_migration::cli::run_migrate(migration::Migrator {}, db, command, cli.verbose)
                .await
                .map_err(|e| anyhow::anyhow!(e.to_string()))
        }
    }
}

fn main() -> anyhow::Result<()> {
    let cargo_manifest_dir = &env::var("CARGO_MANIFEST_DIR").context("should point to a valid manifest dir")?;
    let db_migration_package_path = Path::new(&cargo_manifest_dir)
        .parent()
        .context("should have a parent dir")?
        .join("migration");

    println!(
        "cargo:rerun-if-changed={}",
        db_migration_package_path.join("src").to_string_lossy()
    );
    println!(
        "cargo:rerun-if-changed={}",
        db_migration_package_path.join("Cargo.toml").to_str().unwrap()
    );

    let codegen_path = Path::new(&cargo_manifest_dir)
        .join("src/codegen/postgres")
        .into_os_string()
        .into_string()
        .map_err(|e| anyhow::anyhow!(e.to_str().unwrap_or("illegible error").to_string()))?;

    // Use PostgreSQL URL from environment or default to local instance
    let database_url = env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgresql://bloklid:bloklid@localhost:5432/bloklid_codegen".to_string());

    // Run migrations
    tokio::runtime::Runtime::new()?.block_on(execute_sea_orm_cli_command([
        "sea-orm-cli",
        "migrate",
        "refresh",
        "-u",
        &database_url,
        "-d",
        db_migration_package_path
            .clone()
            .into_os_string()
            .into_string()
            .map_err(|e| anyhow::anyhow!(e.to_str().unwrap_or("illegible error").to_string()))?
            .as_str(),
    ]))?;

    // Generate entities
    tokio::runtime::Runtime::new()?.block_on(execute_sea_orm_cli_command([
        "sea-orm-cli",
        "generate",
        "entity",
        "-o",
        &codegen_path,
        "-u",
        &database_url,
    ]))?;

    Ok(())
}
