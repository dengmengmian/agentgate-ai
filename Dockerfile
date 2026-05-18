# Build stage
FROM rust:1.82-slim AS builder

WORKDIR /build

# Install build dependencies
RUN apt-get update && apt-get install -y pkg-config libssl-dev && rm -rf /var/lib/apt/lists/*

# Copy source
COPY src-tauri/ ./

# Build only the headless binary
RUN cargo build --release --bin agentgate-serve

# Runtime stage
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*

COPY --from=builder /build/target/release/agentgate-serve /usr/local/bin/agentgate-serve

# Default data directory
RUN mkdir -p /data
ENV AGENTGATE_DB_PATH=/data
ENV AGENTGATE_HOST=0.0.0.0
ENV AGENTGATE_PORT=9090

EXPOSE 9090

ENTRYPOINT ["agentgate-serve"]
