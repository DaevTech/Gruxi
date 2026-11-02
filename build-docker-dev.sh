#!/bin/bash

docker pull rust:1.96.0-bookworm
docker pull node:latest
docker pull alpine:3.24

docker buildx build \
  --platform linux/amd64,linux/arm64 \
  -t ghcr.io/daevtech/gruxi:dev \
  --push .