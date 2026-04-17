#!/usr/bin/env bash
# Stop and remove the donto-pg dev container.
set -euo pipefail
NAME=donto-pg
docker rm -f "$NAME" >/dev/null 2>&1 || true
echo "$NAME removed"
