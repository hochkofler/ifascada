# Testing Guide

## Integration Tests

Integration tests are located in `tests/` directories within each crate.

### Infrastructure Tests

The infrastructure crate contains integration tests that require a PostgreSQL database.

#### Setup Test Database

1. **Create test database:**
```bash
createdb ifascada_test
```

2. **Run migrations:**
```bash
psql ifascada_test < migrations/20260210_001_create_tags_schema.sql
```

3. **Set environment variable:**
```bash
# Windows PowerShell
$env:DATABASE_URL="postgres://postgres:password@localhost/ifascada_test"

# Linux/Mac
export DATABASE_URL="postgres://postgres:password@localhost/ifascada_test"
```

#### Running Integration Tests

```bash
# Run all tests
cargo test

# Run only infrastructure integration tests
cargo test --package infrastructure --test tag_repository_tests

# Run specific test
cargo test --package infrastructure --test tag_repository_tests test_save_and_find_tag
```

### Test Structure

```
crates/infrastructure/
├── src/
│   └── database/
│       └── tag_repository/
│           ├── mod.rs
│           └── postgres_tag_repository.rs   # Implementation
└── tests/
    └── tag_repository_tests.rs              # Integration tests (SEPARATED)
```

### Benefits of Separated Tests

✅ **Clear separation** - Tests are not mixed with implementation  
✅ **Integration focus** - Tests use the public API  
✅ **Easy to skip** - Can build without running integration tests  
✅ **Real database** - Tests against actual PostgreSQL 18  

### Test Database Management

**Reset test database:**
```bash
psql ifascada_test < migrations/20260210_001_create_tags_schema_down.sql
psql ifascada_test < migrations/20260210_001_create_tags_schema.sql
```

**Clean up test data:**
Tests automatically clean up data starting with `TEST_` prefix.
