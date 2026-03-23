#!/bin/bash

docker pull rust:alpine
docker pull node:latest
docker pull alpine:latest

docker buildx build \
  --platform linux/amd64,linux/arm64 \
  -t ghcr.io/daevtech/gruxi:dev \
  --push .