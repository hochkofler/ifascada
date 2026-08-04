# Changelog

All notable changes to this project will be documented in this file.

## [1.0.0] - 2026-02-26

### Added
- Initial stable release baseline (`v1.0.0`) for central and edge deployment flows.
- Central deployment using Docker Compose with separated profiles:
  - infrastructure services
  - database seed job
  - central-server runtime
- Reproducible DB seed flow with profiles (`minimal`, `sim20`, `full`).
- Operational deployment documentation for central compose flow.

### Notes
- This is the first formal release entry.
- Future releases should be appended as new sections above this version.
