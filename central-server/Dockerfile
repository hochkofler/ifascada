# central-server/Dockerfile
FROM rust:1-bookworm AS builder
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
RUN cargo build --release --package central-server

FROM debian:bookworm
WORKDIR /app
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/central-server /usr/local/bin/central-server
COPY crates/edge-agent/config /app/config
ENTRYPOINT ["/usr/local/bin/central-server"]
