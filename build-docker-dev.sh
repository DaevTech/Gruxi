#!/bin/bash

docker pull rust:alpine
docker pull node:latest
docker pull alpine:latest

docker buildx build \
  --platform linux/amd64 \
  -t ghcr.io/daevtech/gruxi:dev \
  --push .