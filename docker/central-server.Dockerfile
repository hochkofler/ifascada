FROM rust:1.85-slim AS builder
WORKDIR /app

COPY Cargo.toml Cargo.lock ./
COPY crates/domain ./crates/domain
COPY crates/central-server ./crates/central-server

RUN cargo build -p central-server --release

FROM debian:bookworm-slim
WORKDIR /app
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/central-server /usr/local/bin/central-server
ENTRYPOINT ["/usr/local/bin/central-server"]
