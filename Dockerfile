# Build stage
FROM --platform=$TARGETPLATFORM rust:1.82-slim-bullseye as builder

# Install build dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    curl \
    build-essential \
    protobuf-compiler \
    sqlite3 \
    unzip \
    libsqlite3-dev \
    g++ \
    cmake \
    zlib1g-dev \
    && rm -rf /var/lib/apt/lists/*

# Install sccache using cargo
RUN cargo install sccache

# Set up sccache for Rust
ENV RUSTC_WRAPPER=sccache

# Create a new empty shell project with only Cargo files
WORKDIR /usr/src/nebulous
# Copy Cargo.toml first and handle missing Cargo.lock
COPY Cargo.toml ./
# Create empty Cargo.lock if it doesn't exist
RUN touch Cargo.lock

# Create minimal src directory
RUN mkdir src && echo "fn main() {}" > src/main.rs

# Pre-build dependencies to leverage Docker layer caching
RUN cargo build --release || true

# Now copy actual source code
COPY . .

# Build with release profile, utilizing cache
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/src/nebulous/target \
    cargo build --release

# Runtime stage
FROM debian:bullseye-slim

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    sqlite3 \
    libsqlite3-0 \
    curl \
    unzip \
    && rm -rf /var/lib/apt/lists/*

# Copy the binary from builder
COPY --from=builder /usr/src/nebulous/target/release/nebulous /usr/local/bin/nebu

# Install rclone
RUN curl https://rclone.org/install.sh | bash

# Install AWS CLI
RUN curl "https://awscli.amazonaws.com/awscli-exe-linux-x86_64.zip" -o "awscliv2.zip" && \
    unzip awscliv2.zip && \
    ./aws/install && \
    rm -rf awscliv2.zip aws

# Create directory for SQLite database
RUN mkdir -p /data
WORKDIR /data

# Set environment variables
ENV RUST_LOG=info
ENV DATABASE_URL=sqlite:/data/nebulous.db

# Expose the default port
EXPOSE 3000

# Create a startup script to run the sync tool in the background
RUN echo '#!/bin/bash\n\
nebu sync --config /nebu/sync.yaml --interval-seconds 5 --create-if-missing --watch --background --block-once --config-from-env \n\
exec "$@"' > /entrypoint.sh && \
    chmod +x /entrypoint.sh

ENTRYPOINT ["/entrypoint.sh"]

# Run the binary
CMD ["nebu", "serve", "--host", "0.0.0.0", "--port", "3000"]
