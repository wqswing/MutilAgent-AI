# Builder stage
FROM rust:1-bookworm AS builder
WORKDIR /app

# Build application
COPY . .
RUN cargo build --release --bin opencoordex

# Runtime stage
FROM debian:bookworm-slim AS runtime
WORKDIR /app
RUN apt-get update -y \
    && apt-get install -y --no-install-recommends openssl ca-certificates curl \
    && apt-get autoremove -y \
    && apt-get clean -y \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/opencoordex /usr/local/bin/opencoordex
COPY config /app/config

# Configuration via Env Vars
ENV HOST=0.0.0.0
ENV PORT=3000
ENV RUST_LOG=info

EXPOSE 3000

HEALTHCHECK --interval=30s --timeout=5s --start-period=5s --retries=3 \
    CMD curl -f http://localhost:3000/health || exit 1

ENTRYPOINT ["/usr/local/bin/opencoordex"]
