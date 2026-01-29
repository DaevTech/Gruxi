<img width="200" src="https://github.com/DaevTech/Gruxi/blob/0c198a580b6d473bfbef642301e069507429be26/assets/logo.svg">

# Gruxi — High‑performance web server

Gruxi is an opinionated web server focused on **high performance**, **operational simplicity**, and **predictable behavior**. It is designed to serve as a reliable foundation for modern web applications without exposing users to excessive configuration complexity.

The project is built on practical experience from decades of operating and maintaining production web servers. Gruxi deliberately avoids configuration knobs that rarely provide real‑world value, favoring sensible defaults and a clear administration model instead.

Gruxi is actively developed and tested. New features and improvements are released continuously once they meet stability and quality requirements.

[![MIT License](https://img.shields.io/badge/License-MIT-green.svg)](https://choosealicense.com/licenses/mit/)

---

## Project status

**Status:** Active development

Gruxi is usable today, but it has not yet reached a 1.0 release. Core functionality is stable, while configuration formats and internal APIs may still evolve. Backward‑incompatible changes may occur prior to 1.0.

---

## Features

### Core

* Event‑driven architecture with low per‑request CPU and memory overhead
* Extremely fast static file serving
* High‑performance in‑memory file cache
* Content compression

### Protocols & networking

* HTTP/1.1 and HTTP/2 support
* Reverse proxy with TLS offloading
* Load balancing and health checks

### TLS & security

* Native TLS support
* Automatic certificate issuance and renewal via Let’s Encrypt

### Administration

* Built‑in web interface for administration, configuration, and monitoring
* Live metrics and server status

### Application support

* PHP support via PHP‑FPM
* Managed PHP‑CGI on Windows

---

## Who Gruxi is for — and who it is not

Gruxi is designed for developers, agencies, and hosting environments that value **clarity, performance, and minimal operational overhead**.

Many existing web servers offer extensive configuration surfaces with hundreds of tunables. This flexibility is useful in some cases but often adds unnecessary complexity. In practice, most teams want their web server to behave predictably, perform well, and stay out of the way.

Gruxi intentionally removes low‑value configuration choices. For example, internal buffer sizes and similar micro‑optimizations are not exposed. If a setting is unlikely to materially improve outcomes for the majority of users, it is fixed and carefully chosen.

Gruxi may **not** be a good fit if you:

* Require fine‑grained tuning of internal networking or buffering parameters
* Prefer fully manual TLS or certificate management
* Need a highly extensible plugin ecosystem

---

## Performance

The following section documents **performance characteristics** of Gruxi under controlled load. Benchmarks were executed on local developer hardware with repeatable configurations, without any network overhead beyond localhost. These numbers illustrate Gruxi's potential and should be considered directional; real-world results may vary depending on deployment environment, network conditions, and workload.

### Test environment

* CPU: AMD Ryzen 9 9950X3D 4300MHz
* Memory: DDR5 4800 MHz
* Storage: Samsung 9100 PRO 4TB
* Operating system: Windows 11

### Benchmark setup

* Tooling: Oha ([https://github.com/hatoo/oha](https://github.com/hatoo/oha))
* Tested version: 0.1.7
* TLS: Disabled (to focus on raw request handling)
* File cache: Enabled
* Content type: Static file (default index.html for Gruxi)
* Concurrency: 100
* Request count: 1,000,000
* Operation mode: "ULTIMATE" (request/response logging disabled except for errors)
* Command executed: `.\oha-windows-amd64.exe -c 100 -n 1000000 --no-tui http://127.0.0.1`

### Results

* Requests per second: **173,309 req/second**
* Median latency: **0.005752 ms**
* 99th percentile latency: **0.011570 ms**
* Memory usage under load: **10 MB**
* CPU utilization: **30 %**

### Benchmark screenshot

<img src="https://github.com/DaevTech/Gruxi/blob/main/assets/performance-test-260126-static-files.png" alt="Gruxi Performance Test" width="600">

> **Note:** These results reflect local lab conditions with no external network traffic. Performance may differ under real-world scenarios with TLS enabled, external clients, and varied content types.


---

## Getting started

There are several ways to run Gruxi, depending on your environment and deployment preferences.

### Using prebuilt binaries (recommended for maximum performance)

1. Download the release matching your operating system.
2. Extract the archive; no additional setup is required.
3. Run the Gruxi binary and open [http://localhost](http://localhost) to view the default page.
4. Open the admin portal at [https://localhost:8000](https://localhost:8000) and log in using the username `admin` and the password printed to the console on first startup.

> The initial admin password is shown **only once**. Store it securely.

---

### Running with Docker

Official Docker images are based on Alpine Linux and run Gruxi as a non‑root user.

Basic test run:

```
docker run --name gruxi1 \
  -p 80:80 \
  -p 443:443 \
  -p 8000:8000 \
  -d ghcr.io/daevtech/gruxi:latest
```

Gruxi will be available at [http://localhost](http://localhost) with the admin portal at [https://localhost:8000](https://localhost:8000).

Extended example with persistent data:

```
docker run --name gruxi1 \
  -p 80:80 \
  -p 443:443 \
  -p 8000:8000 \
  -v ./my-web-content:/app/www-default:ro \
  -v ./logs:/app/logs \
  -v ./certs:/app/certs \
  -v ./db:/app/db \
  -d ghcr.io/daevtech/gruxi:latest
```

---

### Docker Compose

For production‑style deployments, Docker Compose is recommended. An example `docker-compose.yml` is included in the repository root.

Minimal example:

```yml
services:
  gruxi:
    image: ghcr.io/daevtech/gruxi:latest
    ports:
      - "80:80"
      - "443:443"
      - "8000:8000"
    volumes:
      - ./db:/app/db
      - ./logs:/app/logs
      - ./certs:/app/certs
# - ./www-default:/app/www-default  # Gruxi serves default site on /app/www-default, so map in your own data her
    restart: unless-stopped
    depends_on:
      - php-fpm
    networks:
      - gruxi-network

  php-fpm:
    image: php:8.2-fpm-alpine
    # volumes:
    # - ./www-default:/var/www/html:ro # Site needs to be available to PHP, so map your own web root to this
    ports:
      - "9000:9000"
    restart: unless-stopped
    networks:
      - gruxi-network

networks:
  gruxi-network:
    driver: bridge
```

---

## Admin portal

The admin portal provides configuration management, monitoring, and operational insight.

* Username: `admin`
* Password: Generated on first startup and printed to the server output

The initial password is not displayed again after first launch. It can be reset using the --reset-admin-password on command line.

---

## Screenshots

![Gruxi startup](https://github.com/DaevTech/Gruxi/blob/main/assets/startup_screenshot.png "Gruxi Admin Portal")

<img src="https://github.com/DaevTech/Gruxi/blob/main/assets/admin_portal_monitoring.png" alt="Gruxi Admin Portal Monitoring" width="600">

<img src="https://github.com/DaevTech/Gruxi/blob/main/assets/admin_portal_configuration.png" alt="Gruxi Admin Portal Configuration" width="600">

---

## Documentation

Comprehensive documentation is available at:

[https://gruxi.org](https://gruxi.org)

---

## Licensing, support, and sponsorship

Gruxi is free to use under the MIT license. Direct support is not included by default.

If you require commercial support, consulting, or wish to sponsor development, please contact:

[contact@gruxi.org](mailto:contact@gruxi.org)

---

## Copyright and Trademark Notice

The source code in this repository is licensed under the [MIT License](LICENSE). You are free to use, modify, and distribute the code in accordance with that license.

However, the following are **not** covered by the MIT License and remain the exclusive property of the Gruxi project and its author:

* The **Gruxi** name and brand
* Logos, icons, and other graphical assets (including files in the `assets/` directory)
* Screenshots and promotional images
* Documentation content and website design at [gruxi.org](https://gruxi.org)

You may not use the Gruxi name, logo, or branding to imply endorsement of or affiliation with your own projects without prior written permission. If you fork or redistribute this software, please rename it and use your own branding.

For licensing inquiries or permission requests, contact: [contact@gruxi.org](mailto:contact@gruxi.org)
