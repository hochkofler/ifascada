//! Integration tests for PostgresTagRepository
//!
//! These tests require a PostgreSQL database.
//! Set DATABASE_URL environment variable to run tests.
//!
//! Example:
//! ```bash
//! export DATABASE_URL="postgres://user:password@localhost/ifascada_test"
//! cargo test --test tag_repository_tests
//! ```

use domain::driver::DriverType;
use domain::tag::TagRepository;
use domain::tag::{ParserConfig, PipelineConfig, TagUpdateMode, TagValueType, ValidatorConfig};
use domain::{Tag, TagId};
use infrastructure::PostgresTagRepository;
use serde_json::json;
use sqlx::PgPool;

/// Helper to create a test database pool
async fn create_test_pool() -> PgPool {
    dotenv::dotenv().ok();
    let database_url =
        std::env::var("DATABASE_URL").expect("DATABASE_URL must be set for integration tests");

    PgPool::connect(&database_url)
        .await
        .expect("Failed to connect to test database")
}

/// Setup test edge agent (required for foreign key)
async fn setup_test_edge_agent(pool: &PgPool, agent_id: &str) {
    sqlx::query!(
        r#"
        INSERT INTO edge_agents (id, description, status, last_heartbeat, created_at, updated_at)
        VALUES ($1, $2, $3, NOW(), NOW(), NOW())
        ON CONFLICT (id) DO NOTHING
        "#,
        agent_id,
        "Test Edge Agent for Integration Tests",
        "online"
    )
    .execute(pool)
    .await
    .expect("Failed to setup test edge agent");
}

/// Helper to clean up test data with a specific prefix
async fn cleanup_test_data(pool: &PgPool, prefix: &str) {
    let pattern = format!("{}%", prefix);
    sqlx::query!("DELETE FROM tags WHERE id LIKE $1", pattern)
        .execute(pool)
        .await
        .expect("Failed to cleanup test data");
}

/// Helper to create a test tag
fn create_test_tag(id: &str, agent_id: &str) -> Tag {
    Tag::new(
        TagId::new(id).unwrap(),
        DriverType::RS232,
        json!({"port": "COM3", "baud_rate": 9600}),
        agent_id.to_string(),
        TagUpdateMode::OnChange {
            debounce_ms: 100,
            timeout_ms: 30000,
        },
        TagValueType::Composite,
    )
}

#[tokio::test]
async fn test_save_and_find_tag() {
    let pool = create_test_pool().await;
    let agent_id = "test-agent-save";
    let prefix = "TEST_SAVE_";
    setup_test_edge_agent(&pool, agent_id).await;
    cleanup_test_data(&pool, prefix).await;

    let repo = PostgresTagRepository::new(pool.clone());
    let tag_id = format!("{}TAG_1", prefix);
    let tag = create_test_tag(&tag_id, agent_id);

    // Save tag
    repo.save(&tag).await.expect("Failed to save tag");

    // Find tag
    let found = repo
        .find_by_id(tag.id())
        .await
        .expect("Failed to find tag")
        .expect("Tag not found");

    assert_eq!(found.id(), tag.id());

    cleanup_test_data(&pool, prefix).await;
}

#[tokio::test]
async fn test_find_nonexistent_tag() {
    let pool = create_test_pool().await;
    let repo = PostgresTagRepository::new(pool);

    let id = TagId::new("NONEXISTENT_TAG_XYZ").unwrap();
    let result = repo.find_by_id(&id).await.expect("Query failed");

    assert!(result.is_none());
}

#[tokio::test]
async fn test_find_by_agent() {
    let pool = create_test_pool().await;
    let agent_id = "test-agent-find";
    let prefix = "TEST_FIND_AGENT_";
    setup_test_edge_agent(&pool, agent_id).await;
    cleanup_test_data(&pool, prefix).await;

    let repo = PostgresTagRepository::new(pool.clone());

    // Create tags for different agents
    let tag1_id = format!("{}TAG_1", prefix);
    let tag2_id = format!("{}TAG_2", prefix);
    let tag1 = create_test_tag(&tag1_id, agent_id);
    let tag2 = create_test_tag(&tag2_id, agent_id);

    repo.save(&tag1).await.expect("Failed to save tag1");
    repo.save(&tag2).await.expect("Failed to save tag2");

    // Find tags for agent-1
    let tags = repo
        .find_by_agent(agent_id)
        .await
        .expect("Failed to find tags");

    // Filter to ensure we only count our test tags (in case of leftovers)
    let my_tags_count = tags
        .iter()
        .filter(|t| t.id().as_str().starts_with(prefix))
        .count();

    assert_eq!(my_tags_count, 2);

    cleanup_test_data(&pool, prefix).await;
}

#[tokio::test]
async fn test_delete_tag() {
    let pool = create_test_pool().await;
    let agent_id = "test-agent-delete";
    let prefix = "TEST_DELETE_";
    setup_test_edge_agent(&pool, agent_id).await;
    cleanup_test_data(&pool, prefix).await;

    let repo = PostgresTagRepository::new(pool.clone());
    let tag_id = format!("{}TAG", prefix);
    let tag = create_test_tag(&tag_id, agent_id);

    // Save and verify exists
    repo.save(&tag).await.expect("Failed to save tag");
    let found = repo.find_by_id(tag.id()).await.expect("Query failed");
    assert!(found.is_some());

    // Delete and verify gone
    repo.delete(tag.id()).await.expect("Failed to delete tag");
    let found = repo.find_by_id(tag.id()).await.expect("Query failed");
    assert!(found.is_none());

    cleanup_test_data(&pool, prefix).await;
}

#[tokio::test]
async fn test_find_enabled_tags() {
    let pool = create_test_pool().await;
    let agent_id = "test-agent-enabled";
    let prefix = "TEST_ENABLED_";
    setup_test_edge_agent(&pool, agent_id).await;
    // Clean up specifically for this test's unique tags
    cleanup_test_data(&pool, prefix).await;

    let repo = PostgresTagRepository::new(pool.clone());

    let tag1_id = format!("{}TAG_VISIBLE", prefix);
    let tag2_id = format!("{}TAG_HIDDEN", prefix);

    let tag1 = create_test_tag(&tag1_id, agent_id);
    let mut tag2 = create_test_tag(&tag2_id, agent_id);
    tag2.disable();

    repo.save(&tag1).await.expect("Failed to save tag1");
    repo.save(&tag2).await.expect("Failed to save tag2");

    let enabled = repo.find_enabled().await.expect("Failed to find enabled");

    // Should only find the enabled tag matching our prefix
    assert!(enabled.iter().any(|t| t.id().as_str() == tag1_id));
    assert!(!enabled.iter().any(|t| t.id().as_str() == tag2_id));

    cleanup_test_data(&pool, prefix).await;
}

#[tokio::test]
async fn test_save_and_load_pipeline_config() {
    let pool = create_test_pool().await;
    let agent_id = "test-agent-pipeline";
    let prefix = "TEST_PIPELINE_";
    setup_test_edge_agent(&pool, agent_id).await;
    cleanup_test_data(&pool, prefix).await;

    let repo = PostgresTagRepository::new(pool.clone());
    let tag_id = format!("{}TAG", prefix);
    let mut tag = create_test_tag(&tag_id, agent_id);

    // Configure pipeline with ScaleParser and RangeValidator
    let mut pipeline = PipelineConfig::default();
    pipeline.parser = Some(ParserConfig::Custom {
        name: "ScaleParser".to_string(),
        config: Some(json!({})),
    });
    pipeline.validators.push(ValidatorConfig::Range {
        min: Some(0.0),
        max: Some(1000.0),
    });
    tag.set_pipeline_config(pipeline);

    // Save
    repo.save(&tag).await.expect("Failed to save tag");

    // Load
    let found = repo
        .find_by_id(tag.id())
        .await
        .expect("Query failed")
        .expect("Tag not found");

    // Verify
    let loaded_pipeline = found.pipeline_config();
    assert!(loaded_pipeline.parser.is_some());

    if let Some(ParserConfig::Custom { name, .. }) = &loaded_pipeline.parser {
        assert_eq!(name, "ScaleParser");
    } else {
        panic!("Expected Custom parser");
    }

    assert_eq!(loaded_pipeline.validators.len(), 1);
    if let ValidatorConfig::Range { min, max } = &loaded_pipeline.validators[0] {
        assert_eq!(*min, Some(0.0));
        assert_eq!(*max, Some(1000.0));
    } else {
        panic!("Expected Range validator");
    }

    cleanup_test_data(&pool, prefix).await;
}
