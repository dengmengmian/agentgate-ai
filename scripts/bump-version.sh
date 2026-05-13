#!/bin/bash
# Usage: ./scripts/bump-version.sh 0.2.0

set -e

NEW_VERSION="$1"

if [ -z "$NEW_VERSION" ]; then
  echo "Usage: $0 <version>"
  echo "Example: $0 0.2.0"
  exit 1
fi

# Validate semver format
if ! echo "$NEW_VERSION" | grep -qE '^[0-9]+\.[0-9]+\.[0-9]+$'; then
  echo "Error: version must be semver format (e.g. 0.2.0)"
  exit 1
fi

ROOT="$(cd "$(dirname "$0")/.." && pwd)"

# 1. package.json
sed -i '' "s/\"version\": \".*\"/\"version\": \"$NEW_VERSION\"/" "$ROOT/package.json"

# 2. src-tauri/Cargo.toml (only the package version line)
sed -i '' "s/^version = \".*\"/version = \"$NEW_VERSION\"/" "$ROOT/src-tauri/Cargo.toml"

# 3. src-tauri/tauri.conf.json
sed -i '' "s/\"version\": \".*\"/\"version\": \"$NEW_VERSION\"/" "$ROOT/src-tauri/tauri.conf.json"

echo "Version updated to $NEW_VERSION in:"
echo "  - package.json"
echo "  - src-tauri/Cargo.toml"
echo "  - src-tauri/tauri.conf.json"
echo "  (Settings & Sidebar read version from Tauri API at runtime)"
echo ""
echo "Next steps:"
echo "  1. Update CHANGELOG.md"
echo "  2. git add -A && git commit -m \"release: v$NEW_VERSION\""
echo "  3. git tag v$NEW_VERSION"
echo "  4. git push origin main --tags"
echo "  5. pnpm tauri build"
