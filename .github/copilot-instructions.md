# Copilot Instructions for the Grux Codebase

## Overview
Grux is a Rust-based, async web server and admin platform with modular request handling, configuration, and database management. It is designed for extensibility (e.g., PHP-CGI support), secure admin endpoints, and efficient static file serving. The project uses SQLite for configuration and user/session data, and supports TLS via Rustls.

## Architecture
- **Entrypoint:** `src/main.rs` initializes logging, loads configuration from SQLite (`db/grux.db`), sets up the database, external request handlers, and launches the HTTP server.
- **Configuration:** Managed via `src/configuration/` module:
  - `src/configuration/load_configuration.rs` — Loads config from SQLite
  - `src/configuration/save_configuration.rs` — Persists config to SQLite
  - `src/configuration/configuration.rs` — Main configuration struct
  - `src/configuration/core.rs`, `binding.rs`, `site.rs`, `request_handler.rs`, etc. — Configuration sub-structures
  - Config is loaded from the `grux_config` table in SQLite. If missing, a default is generated and persisted.
- **HTTP Server:** `src/http/http_server.rs` starts async servers (using `tokio`/`hyper`) for each configured binding. Admin endpoints are always served over TLS via `src/http/http_tls.rs`.
- **Request Handling:**
  - `src/http/handle_request.rs` routes requests to static file serving, admin endpoints, or external handlers (e.g., PHP).
  - `src/external_request_handlers/` contains modular handlers (notably `php_handler.rs` for PHP-CGI via persistent processes).
  - `src/http/file_pattern_matching.rs` handles URL pattern matching for routing.
- **Admin Portal:** Served from `www-admin/` and handled in `src/admin_portal/http_admin_api.rs`. Supports login/logout, config management, and session handling. Source code in `www-admin-src/` (Vite/Vue project).
- **Core Services:** `src/core/` contains essential services:
  - `database_connection.rs`, `database_schema.rs` — Database management
  - `admin_user.rs` — User authentication
  - `background_tasks.rs` — Async background operations
  - `monitoring.rs` — System monitoring
  - `operation_mode.rs`, `command_line_args.rs` — Runtime configuration
- **File Cache:** `src/file/file_cache.rs` implements an in-memory cache for static files, configurable via `src/configuration/file_cache.rs`.
- **Logging:** `src/logging/` module handles logging:
  - `access_logging.rs` — HTTP access logs
  - `buffered_log.rs` — Performance-optimized logging


## Developer Workflows
- **Build:** `cargo build` (or use Docker: `docker-compose build`)
- **Run:** `cargo run` (or `docker-compose up`)
- **Test:** `cargo test` (unit/integration tests in `tests/`)
- **Logs:** Check `logs/` for runtime and trace logs.
- **Admin UI:** Access via `https://localhost:8000` (admin portal, credentials managed in DB)

## Project Conventions & Patterns
- **Configuration is always loaded from SQLite, not from static files.**
- **All request handlers are modular and registered at startup.**
- **Admin endpoints must be served over TLS.**
- **Persistent PHP-CGI processes are managed for PHP request handling.**
- **File cache is optional and configurable.**
- **All code is async-first using `tokio` and `hyper`.**
- **Sensitive data (certs, keys) are stored in `certs/` and mounted in Docker.**
- **Changes to configuration require updating the SQLite DB and reloading the server.**
- **Use Rust's error handling (`Result`, `Option`) extensively for robustness.**
- **Follow Rust's module and naming conventions for clarity.**
- **Changes to configuration struct must also be changed in the admin portal UI code in `www-admin-src/`.**

## Key Files & Directories
### Core Application
- `src/main.rs` — Entrypoint, startup logic
- `src/lib.rs` — Library root
- `src/http/http_server.rs` — Server startup, binding logic
- `src/grux_log.rs` — Logging initialization
- `src/file/file_cache.rs` — Static file cache implementation
- `src/file/file_util.rs` — File system utilities
- `src/grux_port_manager.rs` — Port allocation management

### Configuration (`src/configuration/`)
- `configuration.rs` — Main configuration struct
- `load_configuration.rs` — Loading config from SQLite
- `save_configuration.rs` — Persisting config to SQLite
- `core.rs` — Core server settings
- `binding.rs` — Network binding configuration
- `site.rs` — Site/virtual host configuration
- `request_handler.rs` — Request handler configuration
- `file_cache.rs` — File cache settings
- `gzip.rs` — Compression settings
- `server_settings.rs` — Server-wide settings
- `binding_site_relation.rs` — Binding-to-site mappings

### HTTP Handling (`src/http/`)
- `handle_request.rs` — Main request routing logic
- `http_tls.rs` — TLS/SSL configuration and handling
- `http_util.rs` — HTTP utilities
- `file_pattern_matching.rs` — URL pattern matching

### External Request Handlers (`src/external_request_handlers/`)
- `external_request_handlers.rs` — Handler trait and registry
- `php_handler.rs` — PHP-CGI integration (persistent processes)

### Admin Portal (`src/admin_portal/`)
- `http_admin_api.rs` — Admin API endpoints

### Core Services (`src/core/`)
- `database_connection.rs` — SQLite connection management
- `database_schema.rs` — Database schema definitions
- `admin_user.rs` — User authentication and management
- `background_tasks.rs` — Async background operations
- `monitoring.rs` — System monitoring
- `operation_mode.rs` — Runtime mode configuration
- `command_line_args.rs` — CLI argument parsing

### Logging (`src/logging/`)
- `access_logging.rs` — HTTP access logs
- `buffered_log.rs` — Performance-optimized logging

### Web Content
- `www-admin/` — Admin web UI (built/served statically)
- `www-admin-src/` — Admin UI source code (Vite/Vue project)
  - `src/` — Vue components and logic
  - `package.json`, `vite.config.js` — Build configuration
- `www-default/` — Default web content
- `www-testing/` — Testing content (includes PHP test files)

### Data & Configuration
- `db/` — SQLite database files
- `logs/` — Log output files
- `certs/` — TLS certificates and keys
- `tests/` — Unit and integration tests

### Docker & Deployment
- `docker-compose.yml`, `Dockerfile` — Containerization
- `development/` — Development Docker environments (PHP-FPM, WordPress)

## Examples
- To add a new request handler, implement the trait in `src/external_request_handlers/external_request_handlers.rs` and register it in startup (`src/main.rs`).
- To change admin portal behavior, update `src/admin_portal/http_admin_api.rs` and the UI in `www-admin-src/src/`.
- To adjust file cache, modify config in DB and logic in `src/file/file_cache.rs`.
- To add new configuration options, update structs in `src/configuration/` and corresponding admin UI components.
- To modify logging behavior, update `src/logging/` modules and configuration in `src/grux_log.rs`.

---
For questions or unclear conventions, ask for clarification or check the relevant module's code.
