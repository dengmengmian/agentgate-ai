#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

run() {
  echo
  echo "==> $*"
  "$@"
}

# 顺序按"快且常踩 → 慢"排,快速失败。format:check 与 cargo cli test 是
# 之前漏掉、导致 CI 连红的两项,务必保留。
run pnpm format:check
run pnpm lint
run pnpm build
run cargo fmt --manifest-path src-tauri/Cargo.toml --check
run cargo clippy --manifest-path src-tauri/Cargo.toml --no-default-features --features cli --lib -- -D warnings
run cargo test --manifest-path src-tauri/Cargo.toml --no-default-features --features cli --lib
run node scripts/test-playwright-smoke.mjs
run pnpm vitest run
run pnpm docs:download:check
run pnpm test:quickstart
