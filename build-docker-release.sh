#!/bin/bash

if [ -z "$1" ]; then
  echo "Usage: $0 <version>"
  exit 1
fi

VERSION="$1"

docker buildx build \
  --platform linux/amd64,linux/arm64 \
  -t ghcr.io/daevtech/gruxi:latest \
  -t ghcr.io/daevtech/gruxi:$VERSION \
  --push .