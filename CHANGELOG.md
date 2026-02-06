# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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
