#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT

cp "$ROOT/scripts/release.sh" "$TMP_DIR/release.sh"
cd "$TMP_DIR"
git init -q
git config user.name test
git config user.email test@example.com
printf 'version=v0.1.0\n' > .env
git add -f .env release.sh
git commit -qm initial

ENV_FILE=.env bash release.sh >/dev/null

test "$(cat .env)" = "version=v0.1.1"
test "$(git tag --list v0.1.1)" = "v0.1.1"
git rev-parse -q --verify 'v0.1.1^{tag}' >/dev/null
