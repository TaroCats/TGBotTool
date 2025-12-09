# Stage 1: Builder
FROM rust:1.83-slim-bookworm as builder

# Install build dependencies
RUN apt-get update && \
    apt-get install -y pkg-config libssl-dev && \
    rm -rf /var/lib/apt/lists/*

# Create a new empty shell project
WORKDIR /usr/src/app
RUN cargo new --bin tg_bot_tool

# Copy manifests
COPY Cargo.toml Cargo.lock ./

# Build only the dependencies to cache them
WORKDIR /usr/src/app/tg_bot_tool
# Move manifests to the correct location for the build
RUN mv ../Cargo.toml ../Cargo.lock .

# Build dependencies
RUN cargo build --release

# Remove the dummy source code
RUN rm src/*.rs

# Copy the actual source code
COPY src ./src

# Touch the main file to force a rebuild of the application
RUN touch src/main.rs

# Build the application
RUN cargo build --release

# Stage 2: Runtime
FROM debian:bookworm-slim

# Install runtime dependencies
RUN apt-get update && \
    apt-get install -y ca-certificates libssl3 openssl && \
    rm -rf /var/lib/apt/lists/*

# Copy the binary from the builder stage
COPY --from=builder /usr/src/app/tg_bot_tool/target/release/tg_bot_tool /usr/local/bin/tg_bot_tool

# Set the working directory
WORKDIR /app

# Run the binary
CMD ["tg_bot_tool"]
