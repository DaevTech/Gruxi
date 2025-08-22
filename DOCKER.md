# Grux Docker Setup

This directory contains Docker configuration files for running Grux in a containerized environment.

## Files Overview

- `Dockerfile` - Production Docker image based on Alpine Linux with Rust
- `Dockerfile.dev` - Development image with file watching capabilities
- `docker-compose.yml` - Docker Compose configuration for easy container management
- `docker.sh` / `docker.bat` - Helper scripts for common Docker operations
- `.dockerignore` - Files to exclude from Docker build context

## Prerequisites

- Docker Engine 20.10+
- Docker Compose 2.0+

## Quick Start

### Using Helper Scripts

**Windows:**
```batch
# Build the image
docker.bat build

# Run the container
docker.bat run

# View logs
docker.bat logs -f
```

**Linux/macOS:**
```bash
# Make script executable
chmod +x docker.sh

# Build the image
./docker.sh build

# Run the container
./docker.sh run

# View logs
./docker.sh logs -f
```

### Using Docker Compose Directly

```bash
# Build and run
docker-compose up -d

# View logs
docker-compose logs -f

# Stop
docker-compose down
```

## Port Mapping

The container exposes the following ports:

- **80** - HTTP traffic
- **443** - HTTPS traffic
- **8000** - Admin interface/API

## Volume Mapping

The following directories are mapped between host and container:

- `./src` → `/app/src` (read-only) - Source code
- `./certs` → `/app/certs` - SSL certificates
- `./www-default` → `/app/www-default` - Default web content
- `./www-admin` → `/app/www-admin` - Admin interface
- `./logs` → `/app/logs` - Application logs
- `./temp_test_data` → `/app/temp_test_data` - Test data
- `./grux.db` → `/app/grux.db` - Database file

## Development Mode

For development with automatic rebuilding:

```bash
# Using helper script
./docker.sh dev

# Using docker-compose
docker-compose --profile dev up grux-dev
```

This mode:
- Mounts source code as volumes for live editing
- Uses cargo cache volumes for faster builds
- Automatically rebuilds when files change

## Environment Variables

- `RUST_LOG` - Logging level (default: `info`, dev: `debug`)
- `GRUX_CONFIG_PATH` - Configuration directory path (default: `/app/config`)

## PHP Support

PHP support is handled through a separate PHP-FPM container rather than being included in the Grux container. This provides better separation of concerns and allows for independent scaling and management of PHP processes.

To use PHP with Grux:
1. Set up a separate PHP-FPM container
2. Configure Grux to communicate with PHP-FPM via FastCGI
3. Update your Grux configuration to point to the PHP-FPM container

Example PHP-FPM service in docker-compose.yml:
```yaml
services:
  php-fpm:
    image: php:8.2-fpm-alpine
    volumes:
      - ./www-default:/var/www/html
    networks:
      - grux-network
```

## Configuration

Create a `config` directory in your project root and mount it to `/app/config` to provide custom configuration files.

## Troubleshooting

### Container won't start
- Check if ports 80, 443, or 8000 are already in use
- Verify Docker daemon is running
- Check container logs: `docker-compose logs grux`

### PHP handler not working
- Ensure PHP-FPM container is running and accessible
- Check network connectivity between Grux and PHP-FPM containers
- Verify PHP-FPM configuration and socket/port settings

### Build fails
- Clear Docker cache: `docker system prune -a`
- Check available disk space
- Verify internet connection for dependency downloads

### Permission issues
- Container runs as non-root user (uid: 1000, gid: 1000)
- Ensure mounted directories have appropriate permissions

## Resource Limits

Default resource limits in production:
- Memory limit: 512MB
- Memory reservation: 256MB

Adjust these in `docker-compose.yml` as needed for your environment.

## Security Considerations

- Container runs as non-root user
- Only necessary ports are exposed
- Use proper SSL certificates for HTTPS
- Keep base images updated regularly

## Helper Commands

```bash
# Build image
./docker.sh build

# Run container
./docker.sh run

# Development mode
./docker.sh dev

# Stop containers
./docker.sh stop

# View logs
./docker.sh logs

# Follow logs
./docker.sh logs -f

# Open shell in container
./docker.sh shell

# Clean up everything
./docker.sh clean
```
