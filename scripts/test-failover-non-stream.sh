#!/usr/bin/env bash
set -euo pipefail
echo "=== Failover Test (non-stream via /v1/responses) ==="
echo "Configure multiple providers in Routes page, set mode to failover,"
echo "then run this to test failover behavior."
echo ""
curl -sS -X POST http://127.0.0.1:9090/v1/responses \
  -H "Content-Type: application/json" \
  -d '{
    "model": "gpt-5.5",
    "input": "Say hello in one short sentence.",
    "stream": false
  }' | jq .
