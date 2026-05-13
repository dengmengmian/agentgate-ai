#!/usr/bin/env bash
set -euo pipefail
echo "=== Models List ==="
curl -sS http://127.0.0.1:9090/v1/models | jq .
