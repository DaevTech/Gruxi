<img src="https://github.com/DaevTech/Gruxi/blob/main/assets/github-banner.png" alt="Gruxi Banner">

# Gruxi [![MIT License](https://img.shields.io/badge/License-MIT-green.svg)](https://choosealicense.com/licenses/mit/)

<img src="https://github.com/DaevTech/Gruxi/blob/main/assets/admin_portal_monitoring.png" alt="Gruxi Admin Portal Monitoring" width="400" align="right">

Gruxi is a web server focused on **high performance**, **operational simplicity**, and **predictable behavior**, with a deliberately reduced configuration surface compared to traditional servers like Nginx and Caddy.

A built-in administration web interface eliminates the need for complex configuration files. Gruxi is designed to work well out of the box with minimal tuning, while still providing powerful features for real‑world applications.

The project is built on practical experience from many years of operating and maintaining production web servers and applications hosted on them.

## Key capabilities:

- Built-in admin UI (no external tooling required)
- Handles static files, PHP (via PHP-FPM/CGI), and reverse proxy workloads efficiently
- Minimal configuration model (intentionally reduced tuning surface)

## Features

### Core

- Event‑driven architecture with low per‑request CPU and memory overhead
- Fast static file serving
- High‑performance in‑memory file cache
- Content compression

### Protocols & networking

- HTTP/1.1 and HTTP/2 support
- Reverse proxy with TLS offloading
- Load balancing and health checks
- Caching headers and conditional requests
- Range requests supported

### TLS & security

- Built-in TLS support
- Automatic certificate issuance and renewal via Let’s Encrypt

### Administration

- Built-in web interface for administration, configuration, and monitoring
- Live metrics and server status
- Windows Service support and Linux systemd integration
- Supports Prometheus metrics collection

### Application support

- PHP support via PHP‑FPM
- Managed PHP‑CGI on Windows, with easy version switching

### Logging

- System logging
- Access logging
- Log rotation built‑in, no external tools required

## Who Gruxi is created for

Gruxi is designed for developers, agencies, and hosting environments that value **clarity, performance, and minimal operational overhead**.

Many existing web servers offer extensive configuration surfaces with hundreds of tunables. This flexibility is useful in some cases but often adds unnecessary complexity. In practice, most teams want their web server to behave predictably, perform well, and stay out of the way.

Gruxi may not be the right choice if:

- You require extensive low-level tuning of internal buffers and edge-case behaviors
- You depend on a large ecosystem of third-party modules

## Getting started

There are several ways to run Gruxi, depending on your environment and deployment preferences.

Detailed getting started instructions are available in the [documentation for Gruxi](https://gruxi.org/docs/introduction/getting-started/).

### Quick start options:

- Running with Docker

```sh
docker run --name gruxi1 -p 80:80 -p 443:443 -p 8000:8000 -d ghcr.io/daevtech/gruxi:latest
```

After the container starts, you can access the admin portal at [http://localhost:8000](http://localhost:8000) and the default web content at [http://localhost](http://localhost).

To run it with Docker Compose, check out the [Docker documentation](https://gruxi.org/docs/introduction/getting-started/docker/).

Other installation methods:

- Binaries
    - Download prebuilt binaries for Windows and Linux from the [release page](https://github.com/DaevTech/Gruxi/releases)
    - Run the server directly from the command line with default configuration

- Using Linux packages (.deb, .rpm) - Recommended for production
    - Download packages for your platform from the [release page](https://github.com/DaevTech/Gruxi/releases)
    - Install using your system’s package manager (e.g. `dpkg -i gruxi-1.0.0-amd64.deb` on Debian/Ubuntu)
    - Start the service using `systemd`

## Documentation

Comprehensive documentation is available at:

[https://gruxi.org](https://gruxi.org)

## Admin portal (on port 8000)

The admin portal allows:

- Real-time traffic and performance monitoring
- Configuration management without editing files
- Operational visibility without external tooling

* Username: `admin`
* Password: Generated on first startup and printed to the server output

The initial password is not displayed again after first launch. It can be reset using the `--reset-admin-password` flag via the command line.

## Performance

The following section documents **performance characteristics** of Gruxi under controlled load. Benchmarks were executed on local developer hardware with repeatable configurations, without any network overhead beyond localhost. These numbers illustrate Gruxi's potential and should be considered directional; real-world results may vary depending on deployment environment, network conditions, and workload.

### Test environment

- CPU: AMD Ryzen 9 9950X3D 4300MHz
- Memory: DDR5 4800 MHz
- Storage: Samsung 9100 PRO 4TB
- Operating system: Windows 11

### Benchmark setup

- Tooling: Oha ([https://github.com/hatoo/oha](https://github.com/hatoo/oha))
- Tested version: 1.0.0
- TLS: Disabled (to focus on raw request handling)
- File cache: Enabled
- Content type: Static file (default index.html for Gruxi)
- Concurrency: 100
- Request count: 1,000,000
- Operation mode: "PRODUCTION" (request/response logging disabled except for errors)
- Command executed: `.\oha-windows-amd64.exe -c 100 -n 1000000 --no-tui http://127.0.0.1`

### Results

- Requests per second: **186,918 req/second**
- CPU utilization: **25-30 %**

This level of throughput places Gruxi among high-performance modern web servers under similar conditions.

[Screenshot from Gruxi performance test](https://github.com/DaevTech/Gruxi/blob/main/assets/performance-test-230426-static-files.png)

> **Note:** These results reflect local lab conditions with no external network traffic. Performance will differ under real-world scenarios with TLS enabled, external clients, and varied content types. Much higher performance is possible with optimized configurations and production hardware. These numbers are intended to demonstrate Gruxi's potential and should not be taken as guarantees for all environments.

## Screenshots

![Gruxi startup](https://github.com/DaevTech/Gruxi/blob/main/assets/startup_screenshot.png 'Gruxi Admin Portal')

<img src="https://github.com/DaevTech/Gruxi/blob/main/assets/admin_portal_configuration.png" alt="Gruxi Admin Portal Configuration" width="600" >

## Licensing, support, and sponsorship

Gruxi is free to use under the MIT license. Direct support is not included by default.

If you require commercial support, consulting, or wish to sponsor development, please contact:

[contact@gruxi.org](mailto:contact@gruxi.org)

## Copyright and Trademark Notice

The source code in this repository is licensed under the [MIT License](LICENSE). You are free to use, modify, and distribute the code in accordance with that license.

However, the following are **not** covered by the MIT License and remain the exclusive property of the Gruxi project and its author:

- The **Gruxi** name and brand
- Logos, icons, and other graphical assets (including files in the `assets/` directory)
- Screenshots and promotional images
- Documentation content and website design at [gruxi.org](https://gruxi.org)

You may not use the Gruxi name, logo, or branding to imply endorsement of or affiliation with your own projects without prior written permission. If you fork or redistribute this software, please rename it and use your own branding.

For licensing inquiries or permission requests, contact: [contact@gruxi.org](mailto:contact@gruxi.org)
