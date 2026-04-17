#!/usr/bin/env bash
# Bring up a Postgres 16 container suitable for donto Phase 0 work.
#
# Connection: postgres://donto:donto@127.0.0.1:55432/donto
set -euo pipefail

NAME=donto-pg
PORT=55432
IMAGE=postgres:16

if docker ps --format '{{.Names}}' | grep -qx "$NAME"; then
    echo "$NAME already running on port $PORT"
    exit 0
fi

if docker ps -a --format '{{.Names}}' | grep -qx "$NAME"; then
    docker start "$NAME" >/dev/null
    echo "$NAME restarted on port $PORT"
    exit 0
fi

docker run -d \
    --name "$NAME" \
    -e POSTGRES_USER=donto \
    -e POSTGRES_PASSWORD=donto \
    -e POSTGRES_DB=donto \
    -p "$PORT:5432" \
    "$IMAGE" >/dev/null

echo "waiting for postgres to accept connections..."
for _ in {1..30}; do
    if docker exec "$NAME" pg_isready -U donto -d donto >/dev/null 2>&1; then
        echo "donto-pg ready: postgres://donto:donto@127.0.0.1:$PORT/donto"
        exit 0
    fi
    sleep 1
done

echo "postgres did not become ready in time" >&2
docker logs "$NAME" >&2
exit 1
