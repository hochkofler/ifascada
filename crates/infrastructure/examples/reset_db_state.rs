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

    // Drop all tables in reverse dependency order
    let tables = [
        "report_items",
        "reports",
        "tag_events",
        "tag_history", // legacy – may not exist
        "events",      // legacy – may not exist
        "tags",
        "devices",
        "edge_agents",
        "_sqlx_migrations",
        "seaql_migrations",
    ];

    for table in tables {
        let query = format!("DROP TABLE IF EXISTS {} CASCADE", table);
        match sqlx::query(&query).execute(&pool).await {
            Ok(_) => println!("Dropped table {}", table),
            Err(e) => println!("Note: could not drop {}: {}", table, e),
        }
    }

    println!("✅ Successfully dropped all tables. Run migrations next.");

    Ok(())
}
