use sea_orm_migration::cli;

#[tokio::main]
async fn main() {
    cli::run_cli(migration::Migrator::<0>).await; // 0 = do not populate any v3 Safe data
}
