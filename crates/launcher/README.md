# launcher crate

## Purpose
Small bootstrap executable to load `.env` style variables and start target binaries consistently.

## Typical usage
```powershell
launcher.exe --env-file .env.central -- cargo run -p central-server
launcher.exe --env-file .env.edge-central-com10 -- cargo run -p edge-agent
```

## Notes
1. Keeps deployment simple for operators without manual environment setup.
2. Decouples runtime binary from shell-specific variable export workflows.
