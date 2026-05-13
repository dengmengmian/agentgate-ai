#!/usr/bin/env bash
set -euo pipefail

echo "=== 1. No token → 401 ==="
curl -sS -w "\nHTTP %{http_code}\n" http://127.0.0.1:9090/v1/models 2>&1 || true

echo ""
echo "=== 2. Wrong token → 401 ==="
curl -sS -w "\nHTTP %{http_code}\n" http://127.0.0.1:9090/v1/models \
  -H "Authorization: Bearer wrong_token" 2>&1 || true

echo ""
echo "=== 3. Correct token → 200 ==="
curl -sS http://127.0.0.1:9090/v1/models \
  -H "Authorization: Bearer $(cat ~/.agentgate/token)" | jq .

echo ""
echo "=== 4. /health (no auth needed) → 200 ==="
curl -sS http://127.0.0.1:9090/health | jq .
