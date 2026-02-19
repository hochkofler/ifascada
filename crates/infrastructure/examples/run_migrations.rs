use sqlx::postgres::PgPoolOptions;
use std::env;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Load .env
    dotenv::dotenv().ok();

    let database_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");

    println!("Connecting to database...");
    let pool = PgPoolOptions::new()
        .max_connections(1)
        .connect(&database_url)
        .await?;

    println!("Running migrations...");
    // migrations folder is 2 levels up from crates/infrastructure
    // Adjust path relative to where cargo run is executed (usually root)
    // Or relative to the file? sqlx::migrate! macro uses path relative to the .rs file location usually?
    // Wait, sqlx::migrate! embeds them.
    // Let's try pointing to the migrations folder.
    // crates/infrastructure/examples/run_migrations.rs
    // migrations/ is at root.
    // Relative path: ../../../migrations

    // Note: sqlx::migrate! resolves relative to the file where it is invoked.
    sqlx::migrate!("../../migrations").run(&pool).await?;

    println!("✅ Migrations applied successfully.");

    Ok(())
}
