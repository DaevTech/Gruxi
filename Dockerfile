# Use the latest official Rust image based on Alpine Linux
FROM rust:alpine AS grux-builder

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

# Build the application in release mode
RUN cargo build --release

# Build the admin portal
FROM node:25-alpine3.21 AS admin-portal

WORKDIR /app

COPY www-admin-src/yarn.lock www-admin-src/package.json ./

RUN yarn install --frozen-lockfile

COPY www-admin-src/ ./

RUN yarn run build

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
COPY --from=grux-builder /usr/src/grux/target/release/grux /app/grux

# Copy the built admin portal from the admin-portal stage
COPY --from=admin-portal /www-admin /app/www-admin/

# Create necessary directories and set ownership
RUN mkdir -p /app/logs /app/certs /app/www-default /app/db && \
    chmod 755 /app/certs && \
    chown -R grux:grux /app

# Copy project files and directories (these will be mounted in development)
# But we'll create the structure for when running standalone
COPY  www-default/ /app/www-default/

# Switch to the non-root user
USER grux

# Expose the required ports
EXPOSE 80 443 8000

# Command to run the application
CMD ["./grux"]
