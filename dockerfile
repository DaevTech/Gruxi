# Use the latest official Rust image based on Alpine Linux
FROM rust:alpine as builder

# Install required system dependencies for building
RUN apk add --no-cache \
    musl-dev \
    openssl-dev \
    openssl-libs-static \
    pkgconfig \
    ca-certificates

# Set the working directory inside the container
WORKDIR /usr/src/grux

# Copy the Cargo.toml and Cargo.lock files first for better caching
COPY Cargo.toml Cargo.lock ./

# Copy the source code
COPY src/ ./src/

# Copy additional project files that might be needed
COPY rustfmt.toml ./

# Build the application in release mode
RUN cargo build --release

# Start a new stage for the runtime image
FROM alpine:latest

# Install runtime dependencies
RUN apk add --no-cache \
    ca-certificates

# Create a non-root user for running the application
RUN addgroup -g 1000 grux && \
    adduser -D -s /bin/sh -u 1000 -G grux grux

# Set the working directory
WORKDIR /app

# Copy the compiled binary from the builder stage
COPY --from=builder /usr/src/grux/target/release/grux /app/grux

# Create necessary directories and set ownership
RUN mkdir -p /app/logs /app/certs /app/www-default /app/www-admin /app/temp_test_data && \
    chown -R grux:grux /app

# Copy project files and directories (these will be mounted in development)
# But we'll create the structure for when running standalone
COPY --chown=grux:grux certs/ /app/certs/
COPY --chown=grux:grux www-default/ /app/www-default/
COPY --chown=grux:grux www-admin/ /app/www-admin/

# Switch to the non-root user
USER grux

# Expose the required ports
EXPOSE 80 443 8000

# Set environment variables
ENV RUST_LOG=info
ENV GRUX_CONFIG_PATH=/app/config

# Create a default volume for configuration
VOLUME ["/app/config", "/app/logs", "/app/certs", "/app/www-default", "/app/www-admin"]

# Command to run the application
CMD ["./grux"]
