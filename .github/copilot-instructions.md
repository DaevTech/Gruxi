# Copilot Instructions for the Grux Codebase

## Overview
Grux is a Rust-based, async web server and admin platform with modular request handling, configuration, and database management. It is designed for extensibility (e.g., PHP-CGI support), secure admin endpoints, and efficient static file serving. The project uses SQLite for configuration and user/session data, and supports TLS via Rustls.

## Architecture
- **Entrypoint:** `src/main.rs` initializes logging, loads configuration from SQLite (`grux.db`), sets up the database, external request handlers, and launches the HTTP server.
- **Configuration:** Managed via `src/grux_configuration.rs` and `src/grux_configuration_struct.rs`. Config is loaded from the `grux_config` table in SQLite. If missing, a default is generated and persisted.
- **HTTP Server:** `src/grux_http_server.rs` starts async servers (using `tokio`/`hyper`) for each configured binding. Admin endpoints are always served over TLS.
- **Request Handling:**
  - `src/grux_http_handle_request.rs` routes requests to static file serving, admin endpoints, or external handlers (e.g., PHP).
  - `src/grux_external_request_handlers/` contains modular handlers (notably `grux_handler_php.rs` for PHP-CGI via persistent processes).
- **Admin Portal:** Served from `www-admin/` and handled in `src/grux_http_admin.rs`. Supports login/logout, config management, and session handling.
- **File Cache:** `src/grux_file_cache.rs` implements an in-memory cache for static files, configurable via the core config.
- **Logging:** Uses `log4rs` with logs written to `logs/system.log` and `logs/trace.log`.

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
- `src/main.rs` — Entrypoint, startup logic
- `src/grux_configuration.rs` — Config loading/validation
- `src/grux_http_server.rs` — Server startup, binding logic
- `src/grux_external_request_handlers/` — Modular request handlers (e.g., PHP)
- `src/grux_file_cache.rs` — Static file cache
- `src/grux_http_admin.rs` — Admin portal endpoints
- `www-admin/` — Admin web UI (served statically)
- `www-default/` — Default web content
- `logs/` — Log output
- `certs/` — TLS certificates/keys
- `docker-compose.yml`, `dockerfile` — Containerization

## Examples
- To add a new request handler, implement the trait in `src/grux_external_request_handlers/` and register it in startup.
- To change admin portal behavior, update `src/grux_http_admin.rs` and the UI in `www-admin/`.
- To adjust file cache, modify config in DB and logic in `src/grux_file_cache.rs`.

---
For questions or unclear conventions, ask for clarification or check the relevant module's code.
