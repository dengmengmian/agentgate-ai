#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

run() {
  echo
  echo "==> $*"
  "$@"
}

run pnpm lint
run pnpm build
run node scripts/test-playwright-smoke.mjs
run pnpm vitest run
run pnpm docs:download:check
run pnpm test:quickstart

if [[ "${AGENTGATE_SKIP_DOCKER_PREFLIGHT:-0}" == "1" ]]; then
  echo
  echo "==> Skipping Docker preflight because AGENTGATE_SKIP_DOCKER_PREFLIGHT=1"
  exit 0
fi

if ! command -v docker >/dev/null 2>&1; then
  echo "Docker is required for release-local preflight. Set AGENTGATE_SKIP_DOCKER_PREFLIGHT=1 only for non-release local debugging." >&2
  exit 2
fi

run bash scripts/release-preflight.sh
