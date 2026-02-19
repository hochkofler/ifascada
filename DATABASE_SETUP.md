# Database Setup Guide

## Prerequisites
- PostgreSQL 18 installed
- pgAdmin or psql client

## Step 1: Create Databases

```powershell
# Connect to PostgreSQL
psql -U postgres

# Create production database
CREATE DATABASE ifascada;

# Create test database
CREATE DATABASE ifascada_test;

# Exit
\q
```

## Step 2: Configure Environment Variables

Copy `.env.example` to `.env` and update with your credentials:

```powershell
cp .env.example .env
```

Edit `.env`:
```env
DATABASE_URL=postgres://postgres:YOUR_PASSWORD@localhost:5432/ifascada
TEST_DATABASE_URL=postgres://postgres:YOUR_PASSWORD@localhost:5432/ifascada_test
```

## Step 3: Apply Migrations

### Production Database
```powershell
psql -U postgres -d ifascada -f migrations/20260210_001_create_tags_schema.sql
```

### Test Database
```powershell
psql -U postgres -d ifascada_test -f migrations/20260210_001_create_tags_schema.sql
```

## Step 4: Verify Migration

```powershell
psql -U postgres -d ifascada

# List tables
\dt

# Should show:
# - edge_agents
# - tags
# - tag_history

\q
```

## Step 5: Run Tests

```powershell
# Set environment variable for tests
$env:DATABASE_URL="postgres://postgres:YOUR_PASSWORD@localhost:5432/ifascada_test"

# Run integration tests
cargo test --package infrastructure --test tag_repository_tests
```

## Rollback (if needed)

```powershell
psql -U postgres -d ifascada -f migrations/20260210_001_create_tags_schema_down.sql
```

## Troubleshooting

### Connection refused
- Verify PostgreSQL service is running
- Check port 5432 is accessible

### Authentication failed
- Verify password in .env matches PostgreSQL user password
- Check pg_hba.conf allows md5 or scram-sha-256 authentication

### Permission denied
- Grant permissions: `GRANT ALL PRIVILEGES ON DATABASE ifascada TO postgres;`
