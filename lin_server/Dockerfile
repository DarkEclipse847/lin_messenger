# --- NEW DOCKERFILE ---
FROM rust:1.94-slim-bookworm AS builder
WORKDIR /app
    
# Install system dependencies needed to compile libpq bindings
RUN apt-get update && apt-get install -y \
    libpq-dev \
    pkg-config \
    && rm -rf /var/lib/apt/lists/*
RUN USER=root cargo new --bin lin_server
WORKDIR /app/lin_server
COPY Cargo.toml Cargo.lock ./
RUN cargo build --release
COPY src ./src
RUN touch src/main.rs
RUN cargo build --release
    
FROM debian:bookworm-slim AS runtime
WORKDIR /app
RUN apt-get update && apt-get install -y \
    libpq5 \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/lin_server/target/release/lin_server /app/lin_server
    
EXPOSE 8080

CMD ["/app/lin_server"]