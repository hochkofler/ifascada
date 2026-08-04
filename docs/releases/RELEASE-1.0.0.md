# Release 1.0.0

Release date: 2026-02-26
Version: `1.0.0`

## Summary
Version `1.0.0` establishes the first stable deployment baseline for:
1. Central server deployment using Docker Compose.
2. Edge deployment as OS-native installation/service.

## Included scope
1. Central infrastructure stack (PostgreSQL/Timescale, Redis, Mosquitto).
2. Central runtime service (`central-server`) deployment profile.
3. Database migration and seed execution profile.
4. Seed profiles for different scenarios:
   - `minimal`
   - `sim20`
   - `full`

## Deployment model
1. Central and edge are deployed independently.
2. Central runtime uses Compose profiles to separate concerns:
   - Infra lifecycle
   - DB schema/seed execution
   - Runtime startup
3. Edge runtime remains decoupled from central deployment lifecycle.

## Operational notes
1. Use fixed image versions in production (avoid `latest`).
2. Execute DB seed as a controlled step, not on every restart.
3. Keep environment variables externalized (`.env` / service env files).
4. Validate health and logs after deployment.

## Breaking changes
None for this baseline release.

## Rollback strategy
1. Pin previous image tag in deployment env.
2. Recreate only affected services.
3. Restore DB backup when schema/data rollback is required.
