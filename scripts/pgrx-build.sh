#!/usr/bin/env bash
# Build and test the pg_donto extension end-to-end inside a container.
# No host sudo, no host pg-dev headers.
#
#   ./scripts/pgrx-build.sh [pg_version]
#
# pg_version defaults to 16. Supported: 13, 14, 15, 16, 17.
set -euo pipefail

PG_VERSION="${1:-16}"
TAG="donto-pgrx-pg${PG_VERSION}"

echo "Building $TAG (postgres ${PG_VERSION})..."
docker build \
    --build-arg "PG_VERSION=${PG_VERSION}" \
    -t "$TAG" \
    -f crates/pg_donto/Dockerfile \
    .

echo "Running pgrx test suite..."
docker run --rm "$TAG"
