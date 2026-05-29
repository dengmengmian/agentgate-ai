#!/usr/bin/env bash
set -euo pipefail
echo "=== Chat Completions Pass-Through (non-stream) ==="
curl -sS -X POST http://127.0.0.1:9090/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{
    "model": "gpt-5.5",
    "messages": [
      {
        "role": "user",
        "content": "Say hello in one short sentence."
      }
    ],
    "stream": false
  }' | jq .
