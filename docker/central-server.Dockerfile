FROM rust:1-bookworm AS builder
WORKDIR /app
ENV CARGO_TARGET_DIR=/app/target

COPY crates/domain ./crates/domain
COPY crates/central-server ./crates/central-server
COPY Cargo.lock ./crates/central-server/Cargo.lock

RUN cargo build --manifest-path crates/central-server/Cargo.toml --release

FROM debian:bookworm-slim
WORKDIR /app
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/central-server /usr/local/bin/central-server
COPY crates/edge-agent/config /app/config
ENTRYPOINT ["/usr/local/bin/central-server"]
