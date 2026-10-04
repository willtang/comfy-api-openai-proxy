# Multi-stage Dockerfile for comfy-api-openai-proxy

# --- Build Stage ---
FROM rust:1.80-slim-bookworm AS builder

WORKDIR /usr/src/app

# Install build dependencies
RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config \
    libssl-dev \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Cache dependency layer (Cargo.lock* is optional so builds succeed with or without it)
COPY Cargo.toml Cargo.lock* ./
# Create dummy src/main.rs and templates for cargo build cache
RUN mkdir src && echo "fn main() {}" > src/main.rs && \
    mkdir templates && echo "{}" > templates/txt2img.json && echo "{}" > templates/img2img.json && \
    cargo build --release && \
    rm -rf src templates

# Copy actual source code and templates
COPY src ./src
COPY templates ./templates

# Ensure cargo detects source changes and builds the binary
RUN touch src/main.rs && cargo build --release

# --- Runtime Stage ---
FROM debian:bookworm-slim AS runner

WORKDIR /app

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    libssl3 \
    curl \
    && rm -rf /var/lib/apt/lists/*

# Copy the compiled binary from the builder stage
COPY --from=builder /usr/src/app/target/release/comfy-api-openai-proxy /usr/local/bin/comfy-api-openai-proxy

# Copy default templates
COPY templates ./templates

# Environment defaults
ENV HOST=0.0.0.0 \
    PORT=3000 \
    COMFY_BASE_URL=http://host.docker.internal:8189 \
    TXT2IMG_TEMPLATE_PATH=/app/templates/txt2img.json \
    IMG2IMG_TEMPLATE_PATH=/app/templates/img2img.json

EXPOSE 3000

CMD ["comfy-api-openai-proxy"]
