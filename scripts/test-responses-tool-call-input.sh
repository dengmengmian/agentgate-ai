#!/usr/bin/env bash
set -euo pipefail
echo "=== Tool Call Input (multi-turn) ==="
curl -sS -X POST http://127.0.0.1:9090/v1/responses \
  -H "Content-Type: application/json" \
  -d '{
    "model": "gpt-5.5",
    "input": [
      {
        "type": "message",
        "role": "user",
        "content": [
          {
            "type": "input_text",
            "text": "Use the tool result and summarize it."
          }
        ]
      },
      {
        "type": "function_call",
        "call_id": "call_test_1",
        "name": "read_file",
        "arguments": "{\"path\":\"README.md\"}"
      },
      {
        "type": "function_call_output",
        "call_id": "call_test_1",
        "output": "# AgentGate\nLocal gateway for AI coding agents."
      }
    ],
    "stream": false
  }' | jq .
