<img width="200" src="https://github.com/DaevTech/Gruxi/blob/0c198a580b6d473bfbef642301e069507429be26/assets/logo.svg">

#  Gruxi - High performance web server

Gruxi is a web server focused on high performance and ease of administration. It has a built-in web interface to change settings, add websites etc. No more need for complicated configuration files with weird syntax. Written in high performance Rust. Supports PHP right out of the box.

Gruxi is actively being developed & tested, so we are rolling out improvements and new features whenever we have a stable version.

[![MIT License](https://img.shields.io/badge/License-MIT-green.svg)](https://choosealicense.com/licenses/mit/)

## Features

- Serve files in fastest possible manner
- Really low memory footprint and low CPU usage per request
- Easy web interface for administration of everything built right in
- SSL/TLS support for secure https:// sites
- Supports HTTP1.1 and HTTP2.0
- PHP Support (both FPM and php-cgi on Windows (needs to be v7.1+ of PHP for Windows))
- High performance file cache to really speed up
- Gzip of content, to make it as small as possible (cached ofcourse)
- Monitoring of current load and state directly from the admin portal

## Getting started

To get started running Gruxi, there is a few options:

### Using the binaries directly

1. Download the release appropriate for the system you want to run it on.
2. This is a ready to go build right after it is extracted.
3. Run the binary and check out http://localhost for the default Gruxi page.
4. To do configuration, go to https://localhost:8000 and login with the user "admin" and the password given in the output from the server on first run. Save it, as it will NOT be shown again.

### With Docker:

1. Make sure Docker is installed
2. Open your terminal of choice (such as **bash** on Linux and **terminal** on Windows)
3. Basic test: `docker run --name gruxi1 -p 80:80 -p 443:443 -p 8000:8000 -d ghcr.io/daevtech/gruxi:latest`
4. Gruxi is now available on http://localhost and admin portal on https://localhost:8000
5. Extended example to map in your own content:

`docker run --name gruxi1 -p 80:80 -p 443:443 -p 8000:8000 -v ./my-web-content:/app/www-default:ro -v ./logs:/app/logs -v ./certs:/app/certs -v ./db:/app/db -d ghcr.io/daevtech/gruxi:latest`

### With Docker Compose

To better control the deployment of Gruxi instead of long docker commands, check out the docker-compose.yml in the Gruxi github repository root. That shows example to have persistent database, logs, certificates etc. This is really the way to go. So download it and adjust for your need.

Run it in a terminal with `docker compose up -d` in the same place as the docker-compose.yml is.

This is a basic example:
```yml
services:
  gruxi:
    image: ghcr.io/daevtech/gruxi:latest
    ports:
      - "80:80"     # HTTP
      - "443:443"   # HTTPS
      - "8000:8000" # Admin/API port
    volumes:
      # Mount project directories for development
      - ./db:/app/db
      - ./logs:/app/logs
      - ./certs:/app/certs
#      - ./www-default:/app/www-default
    restart: unless-stopped
    depends_on:
      - php-fpm
    networks:
      - gruxi-network

  # PHP-FPM service for handling PHP requests
  php-fpm:
    image: php:8.2-fpm-alpine
#    volumes:
#      - ./www-default:/var/www/html:ro# Web content accessible to PHP-FPM
    ports:
      - "9000:9000"
    restart: unless-stopped
    networks:
      - gruxi-network

networks:
  gruxi-network:
    driver: bridge

```



## Admin portal

Log in to admin portal with "admin" as username and password written in the server output.

This auto-generated password is only written on first startup, so note it down somewhere safe.

## Screenshots

![Screenshot of start up](https://github.com/DaevTech/Gruxi/blob/main/assets/startup_screenshot.png "Gruxi Admin Portal")

<img src="https://github.com/DaevTech/Gruxi/blob/main/assets/admin_portal_monitoring.png" alt="Gruxi Admin Portal Monitoring" width="600">

<img src="https://github.com/DaevTech/Gruxi/blob/main/assets/admin_portal_configuration.png" alt="Gruxi Admin Portal configuration" width="600">


## Documentation

[You can find documentation for Gruxi web server here](https://gruxi.org)


## Help with development

Do you want to help with the development and build Gruxi locally. It is easy.

### Using Rust framework:

1. Install rust framework - https://rust-lang.org/tools/install/
2. Clone Gruxi repository with git
3. Build gruxi by running: "cargo run -- -o DEV" (this will run it in dev mode, with trace log enabled)

If you want admin portal running, you need to build that too.

1. Install node.js
2. Go into /www-admin-src
3. Run "npm run build"

Gruxi can now be found on http://localhost and admin portal on https://localhost:8000

### Easy mode development with Docker compose:
If you rather want total easy mode development, use the docker solution:

1. Install docker
2. Clone Gruxi repository with git
3. Go into /development
4. Run "docker compose up -d"
5. After a while, Gruxi is running on http://localhost and admin portal on https://localhost:8000

Log in to admin portal with "admin" as username and password written in the server output. Only written on first startup.

After your changes is done, make sure it builds and tests are running.
Submit a PR and wait for approval. We appreciate any contribution and improvements.

## Licensing with support or sponsoring

Gruxi is free to use for everybody, but does not provide direct support. If you need support for private or commercial context or want to sponsor the project, let us know and we will figure out a solution. Contact us on <contact@gruxi.org>.

## Authors

[Brian Søgård Jensen](https://www.github.com/briansjensen) - Contact info: <bsj@succesprojekter.dk>
