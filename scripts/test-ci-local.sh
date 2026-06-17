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
