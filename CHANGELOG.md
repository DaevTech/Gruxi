# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

# Version 1.1.0 - 24 June 2026

### Added
 - #29 - Add CSRF protection to the admin portal.
 - #33 - Add a short-lived cache for frequently accessed files when the main file cache is disabled.
 - #33 - Add short-lived cache configuration options and expose cache usage details in the admin dashboard.
 - #5 - Add an option in the admin portal to disable admin authentication for local development and similarly safe environments.
 - #5 - Add file-not-found cache visibility to the dashboard and group cache-related settings under a new `Caching` configuration section.

### Changed
 - #32 - Improve compression performance significantly by switching gzip compression to `zlib-rs`.
 - #32 - Improve compression handling without the main file cache by introducing a hot-file short-lived compression cache.
 - #36 - Replace the primary file cache implementation with Moka and simplify cache management.
 - #36 - Simplify cache-related configuration by removing eviction-threshold and cache update interval settings.
 - #36 - Simplify version handling by keeping a single version instead of separate configuration and database schema versions.
 - #17 - Change the file reader cache so concurrent requests for the same file share a single file system read, preventing cache stampedes.
 - #17 - Optimize cache-control and compressible mime-type handling by caching those lookups.
 - #27 - Refresh the default HTML page and add a link to the administration portal for easier first-time setup.
 - Improve monitoring and metrics exporter internals, and add a bit more tracing to monitoring.
 - Bump the minimum Rust toolchain used in development to 1.96.
 - Comment out benchmarks by default to avoid Docker build issues.

### Fixed
 - #37 - Add more validation for the `php-cgi` field in managed external systems.
 - Apply dependency updates and related corrections, including a protobuf security-related update.
 - Include a handful of smaller fixes and maintenance cleanups shipped alongside the larger changes.


# Version 1.0.2 - 22 May 2026

Bug fix release

### Fixed
 - #28 - Make sure gzip compression is done, even if data is streaming and file cache is disabled
 - #30 - Fix the proxy processor rewriting urls wrong in certain cases, where uri returns a full url and not just the path and query.
 - #31 - Fix x-forwarded-host, which should contain port if non-standard. Also x-forwarded-for should only contain raw ip and not any ports.

# Version 1.0.1 - 06 May 2026

Bug fix release

### Fixed
- #25 Fixed a issue in the static file processor, not correctly serving index file in subdirectories.
- #3 Fixed a minor issue where username field did not have focus on login page of admin portal, which made it a bit less user friendly.

# Version 1.0.0 - 23 Apr 2026

This is the first stable release of Gruxi, marking a significant milestone in its development. This release includes a wide range of features, improvements, and optimizations that have been implemented and tested over the course of development. The focus has been on performance, security, and usability, making Gruxi a robust and reliable web server for production use.

### Added
 - Added a specific 404 cache, to cache 404 response for a short time

### Fixed
 - Fixed some minor issues and a few bugs following the extensive automated and manual testing that was done for this release, to ensure the highest possible quality and stability for production use.


# Version 0.5.0 - 7 Apr 2026

### Added
 - Added support for Prometheus metrics collection, so you can easily monitor Gruxi with your existing Prometheus setup and get insights into its performance and behavior. Currently implemented metrics are the same as the ones available in the admin portal, but now they can be collected and visualized in Prometheus and Grafana or similar tools.
 - Added RPM packaging for easy installation on Red Hat-based systems, making it more convenient to deploy Gruxi in enterprise environments that use RPM for package management.
 - Make it possible to change the password of the "admin" user in the admin portal, so you can easily set a custom password after the initial startup without having to use command line options or similar.

### Fixed
 - Fixed a issue where the server reload would fail when doing configuration editing in the admin portal.


# Version 0.4.0 - 20 Mar 2026

### Added
 - Add security HTTP headers to admin portal, to make it as secure as possible.
 - Prevent multiple login sessions for the same user in admin portal, to prevent security issues.
 - Better trailing slash normalization - Add slash for directory requests and give error for file requests with trailing slash, to prevent confusion and potential security issues.
 - Add support for canonical hostname, so you can specify which hostname should be dominant for a site, so proper redirects can be made and to prevent duplicate content issues.
 - Add support for enforcing TLS for a site and on a custom port (if different from 443), so you can easily enforce secure connections for your sites.
 - Make it possible to start Gruxi with a configuration JSON file as parameter (-c) or be automatically loaded when named gruxi_config.json and placed in the working directory. This makes it possible to keep a config in git or similar and load that on both for binaries and in Docker. See documentation on configuration for more details.

### Changed
 - Make sure the admin portal password is not logged to log file, but only printed to console on startup.
 - Command line flags --install-service and --remove-service is now only available on Windows. On other platforms, running with systemd is the recommended way to run Gruxi as a service.


# Version 0.3.0 - 3 Mar 2026

### Added

- Added .deb packaging for easy installation on Debian-based systems
- Added Windows service support, so you can run Gruxi as a service on Windows without extra tools
    - Added `--install-service` and `--remove-service` command line options for managing the Windows service
- Added support for running Gruxi as a systemd service on Linux, with a provided systemd unit file for easy setup
- Add rate limiting on the admin portal, to prevent brute-force attacks on the admin password

### Changed

- Improve the access logging, to not be computed on the hot path, but instead be computed in a separate thread after the response is sent, so it doesnt affect performance as much, especially under extreme load

### Fixed

- Fixed a issue where response length would show as 0 in the access logs for certain types of requests, which made it harder to analyze traffic patterns and detect potential issues

# Version 0.2.0 - 6 Feb 2026

### Added

- Add task id in the syslog, so we can correlate logs for the same connection more easily
- Add log rotation directly in Gruxi, so we dont have to rely on external tools for that, and it works on all platforms without extra configuration
- Add a cache clear button in the admin portal, so you can clear the file cache without restarting the server and with ease
- Add a max connection duration settings, so we can automatically close connections that have been open for too long, which can help with certain types of attacks and also just free up resources in case of hanging connections

### Changed

- Improve the monitoring to run every second and track only non-admin requests, so we can have more accurate and real-time monitoring of the server status without the noise of admin portal requests

# Version 0.1.8 - 2 Feb 2026

### Added

- Add build to more platforms in GitHub for our binaries, so we now build for Linux (x64 and arm64), Windows and MacOS (amd64 and arm64)
- Add multi platform docker build support, so our docker image now works on both x64 and arm64 systems
- Range Request support for static files
- HTTP Caching headers
    - `ETag` header
    - `Last-Modified` header
    - `Cache-Control` header
    - `Expires` header
- Support for conditional requests using `If-Match`, `If-None-Match`, `If-Modified-Since` and `If-Unmodified-Since` headers
- Add timestamp for last updated in admin portal, along with a dropdown to select refresh rate on status page
- Vastly improved logging performance, for syslog and access logging, so it doesnt affect performance as much anymore, especially under extreme load

### Removed

- Alternative file path for certificate files (was determined to be the fluff Gruxi is trying to avoid)
- We had two different intervals defined for the file cache in the configuration, which was stupid as we only had one clean-up thread. Now there's only one.

### Fixed

- Fix request counter not decrementing in all cases, leading it to never actually reset, but keep rising a little bit every day
- Fixed issue where missing TLS certificate files would cause server to stop on startup

# Version 0.1.7 - 25 Jan 2026

### Added

- Introduce painless TLS with LetsEncrypt using the TLS-ALPN-01 challenge
- Add server software spoof field for PHP

# Previous releases < 0.1.7 can be found on GitHub

We have decided to not maintain changelogs for versions prior to 0.1.7 in this file, as the project was in a very early stage and underwent significant changes that made previous changelogs less relevant. Aka, nobody really cares.
