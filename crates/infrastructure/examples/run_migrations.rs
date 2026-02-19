use sea_orm_migration::prelude::*;
use std::env;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Load .env
    dotenv::dotenv().ok();

    let database_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");

    println!("Connecting to database...");
    let connection = sea_orm::Database::connect(&database_url).await?;

    println!("Running migrations...");
    migration::Migrator::up(&connection, None).await?;

    println!("✅ Migrations applied successfully.");

    Ok(())
}
