use sea_orm_migration::cli;

#[tokio::main]
async fn main() {
    cli::run_cli(migration::Migrator::<1>).await;
}
