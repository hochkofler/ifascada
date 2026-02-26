FROM rust:1.87-bookworm AS builder
WORKDIR /src

COPY Cargo.toml Cargo.lock ./
COPY crates ./crates

RUN cargo build --release -p edge-agent

FROM debian:bookworm-slim
WORKDIR /app

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /src/target/release/edge-agent /usr/local/bin/edge-agent
COPY crates/edge-agent/config /app/config

ENTRYPOINT ["edge-agent"]
