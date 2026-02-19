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

    let tables = [
        "tag_events",
        "events",
        "tag_history",
        "tags",
        "edge_agents",
        "_sqlx_migrations",
    ];

    for table in tables {
        let query = format!("DROP TABLE IF EXISTS {} CASCADE", table);
        sqlx::query(&query).execute(&pool).await?;
        println!("Dropped table {}", table);
    }

    println!("Successfully dropped all tables.");

    Ok(())
}
