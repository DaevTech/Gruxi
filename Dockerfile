# syntax=docker/dockerfile:1.7

############################
# Rust builder
############################
FROM --platform=$TARGETPLATFORM rust:1.96.0-alpine3.24 AS gruxi-builder

RUN apk add --no-cache \
    musl-dev \
    openssl-dev \
    openssl-libs-static \
    pkgconfig \
    ca-certificates

WORKDIR /usr/src/gruxi

COPY Cargo.toml Cargo.lock ./
COPY src/ ./src/

RUN cargo build --release && \
    cp target/release/gruxi /usr/src/gruxi/gruxi

############################
# Admin portal builder
############################
FROM --platform=$BUILDPLATFORM node:24-alpine3.22 AS admin-portal

WORKDIR /app

COPY www-admin-src/package.json www-admin-src/package-lock.json ./
RUN npm ci

COPY www-admin-src/ ./
RUN npm run build

############################
# Runtime image
############################
FROM alpine:3.24

RUN apk add --no-cache ca-certificates

# Non-root user
RUN addgroup -g 1000 gruxi && \
    adduser -D -s /bin/sh -u 1000 -G gruxi gruxi

WORKDIR /app

# Copy Gruxi binary
COPY --from=gruxi-builder /usr/src/gruxi/gruxi /app/gruxi

# Copy admin portal
COPY --from=admin-portal /www-admin /app/www-admin/

# Create directories
RUN mkdir -p /app/logs /app/certs /app/www-default /app/db && \
    chmod 755 /app/certs && \
    chown -R gruxi:gruxi /app

COPY www-default/ /app/www-default/

USER gruxi

EXPOSE 80 443 8000 8001

CMD ["./gruxi"]
