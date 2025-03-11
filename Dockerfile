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

# Create a new empty shell project
WORKDIR /usr/src/nebulous
COPY . .

# Build with release profile
RUN cargo build --release

# Runtime stage
FROM debian:bullseye-slim

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    sqlite3 \
    libsqlite3-0 \
    && rm -rf /var/lib/apt/lists/*

# Copy the binary from builder
COPY --from=builder /usr/src/nebulous/target/release/nebulous /usr/local/bin/nebu

RUN curl https://rclone.org/install.sh | bash

RUN apt-get update && apt-get install -y \
    ca-certificates \
    curl \
    unzip \
    && rm -rf /var/lib/apt/lists/*


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
if [ ! -z "$NEBU_SYNC_CONFIG" ]; then\n\
  mkdir -p /workspace\n\
  echo "$NEBU_SYNC_CONFIG" > /workspace/sync.yaml\n\
fi\n\
nebu sync --config /workspace/sync.yaml --interval 5 --create-if-missing --watch &\n\
exec "$@"' > /entrypoint.sh && \
    chmod +x /entrypoint.sh

ENTRYPOINT ["/entrypoint.sh"]

# Run the binary
CMD ["nebu", "serve", "--host", "0.0.0.0", "--port", "3000"]