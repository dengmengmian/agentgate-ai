#!/usr/bin/env bash
set -euo pipefail
echo "=== Stream Responses ==="
curl -N -X POST http://127.0.0.1:9090/v1/responses \
  -H "Content-Type: application/json" \
  -d '{
    "model": "gpt-5.5",
    "input": "Say hello in one short sentence.",
    "stream": true
  }'
