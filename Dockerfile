# Build Dashboard Stage
FROM node:20-alpine as dashboard-builder
WORKDIR /usr/src/app/web-dashboard
COPY web-dashboard/package*.json ./
RUN npm install
COPY web-dashboard/ .
RUN npm run build

# Build Rust Stage
FROM rust:latest as rust-builder
WORKDIR /usr/src/app
RUN apt-get update && apt-get install -y libssl-dev pkg-config
ENV SQLX_OFFLINE=true
COPY . .
RUN cargo build --release --workspace

# Runtime Stage - Central Server
FROM debian:bookworm-slim as central-server
RUN apt-get update && apt-get install -y libssl-dev ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=rust-builder /usr/src/app/target/release/central-server /usr/local/bin/central-server
COPY --from=dashboard-builder /usr/src/app/web-dashboard/dist/web-dashboard/browser /app/static
WORKDIR /app
EXPOSE 3000
CMD ["central-server", "--api-port", "3000"]

# Runtime Stage - Edge Agent
FROM debian:bookworm-slim as edge-agent
RUN apt-get update && apt-get install -y libssl-dev ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=rust-builder /usr/src/app/target/release/edge-agent /usr/local/bin/edge-agent
COPY --from=rust-builder /usr/src/app/config /app/config
WORKDIR /app
CMD ["edge-agent"]
