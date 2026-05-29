#!/usr/bin/env bash
set -euo pipefail
echo "=== Failover Test (non-stream via /v1/chat/completions) ==="
curl -sS -X POST http://127.0.0.1:9090/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{
    "model": "gpt-5.5",
    "messages": [
      {"role": "user", "content": "Say hello in one short sentence."}
    ],
    "stream": false
  }' | jq .
