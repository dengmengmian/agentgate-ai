# Build stage
FROM rust:1-bookworm AS builder

WORKDIR /build

# Install build dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    libwebkit2gtk-4.1-dev \
    libappindicator3-dev \
    librsvg2-dev \
    patchelf \
    && rm -rf /var/lib/apt/lists/*

# Copy source
COPY src-tauri/ ./

# Build only the headless binary
RUN cargo build --release --bin agentgate-serve

# Runtime stage
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y \
    ca-certificates \
    libwebkit2gtk-4.1-0 \
    libappindicator3-1 \
    librsvg2-2 \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /build/target/release/agentgate-serve /usr/local/bin/agentgate-serve

# Default data directory
RUN mkdir -p /data
ENV AGENTGATE_DB_PATH=/data
ENV AGENTGATE_HOST=0.0.0.0
ENV AGENTGATE_PORT=9090

EXPOSE 9090

ENTRYPOINT ["agentgate-serve"]
