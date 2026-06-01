#!/usr/bin/env bash
# Two-legged smoke runner for AgentGate releases:
#   Leg 1 — offline fixture tests (no keys, no network) — must always pass.
#   Leg 2 — real provider smoke against the local DB — opt-in via env.
#
# Usage:
#   bash scripts/release-smoke.sh                      # leg 1 only
#   AG_RUN_SMOKE_TESTS=1 bash scripts/release-smoke.sh # both legs
#
# Leg 2 reads provider IDs from the env matrix documented in
# src-tauri/tests/smoke_test.rs (AG_SMOKE_ANTHROPIC_PROVIDER_ID, etc.).
# Missing entries skip the corresponding sub-case but do not fail.
set -euo pipefail

cd "$(dirname "$0")/../src-tauri"

echo "── 1/2: Offline fixture tests (capability + protocol) ───"
cargo test \
  --test capability_fixture \
  --test mimo_capabilities \
  --test deepseek_capabilities \
  --test kimi_capabilities \
  --test protocol_fixture
echo

if [ "${AG_RUN_SMOKE_TESTS:-0}" != "1" ]; then
  echo "── 2/2: Real provider smoke ─────────────────────────────"
  echo "⚠️  Skipped. Set AG_RUN_SMOKE_TESTS=1 (and AG_SMOKE_* provider envs)"
  echo "    to run the real-upstream verification against your local DB."
  exit 0
fi

echo "── 2/2: Real provider smoke (hitting upstream providers) ─"
cargo test --test smoke_test -- --nocapture
