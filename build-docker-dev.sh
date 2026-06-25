#!/bin/bash

docker pull rust:1.96.0-alpine3.24
docker pull node:latest
docker pull alpine:3.22

docker buildx build \
  --platform linux/amd64 \
  -t ghcr.io/daevtech/gruxi:dev \
  --push .