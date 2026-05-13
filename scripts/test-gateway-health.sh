#!/usr/bin/env bash
set -euo pipefail
echo "=== Health Check ==="
curl -sS http://127.0.0.1:9090/health | jq .
