#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PYTHON_BIN="${PYTHON:-python3}"
VERSION_TOOL="$ROOT/scripts/repo_version.py"
VERSION_FILES=(
  .env.example package.json package-lock.json pyproject.toml uv.lock
  src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/tauri.conf.json
)
cd "$ROOT"

"$PYTHON_BIN" "$VERSION_TOOL" --root "$ROOT" get >/dev/null

dirty="$(git status --porcelain --untracked-files=normal)"
if [[ -n "$dirty" ]]; then
  echo "release: worktree must be clean:" >&2
  printf '%s\n' "$dirty" >&2
  exit 1
fi

if [[ -n "${V:-}" ]]; then
  NEW_VERSION="${V//$'\r'/}"
else
  NEW_VERSION="$("$PYTHON_BIN" "$VERSION_TOOL" --root "$ROOT" get --bump-patch)"
fi
if [[ ! "$NEW_VERSION" =~ ^v(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$ ]]; then
  echo "release: expected V=vX.Y.Z, got ${NEW_VERSION:-<empty>}" >&2
  exit 1
fi
if git show-ref --verify --quiet "refs/tags/$NEW_VERSION"; then
  echo "release: git tag already exists: $NEW_VERSION" >&2
  exit 1
fi

"$PYTHON_BIN" "$VERSION_TOOL" --root "$ROOT" set "$NEW_VERSION"
"$PYTHON_BIN" "$VERSION_TOOL" --root "$ROOT" check "$NEW_VERSION"
git add -- "${VERSION_FILES[@]}"
if ! git diff --cached --quiet --; then
  git commit -m "chore: release $NEW_VERSION" -- "${VERSION_FILES[@]}"
fi
git tag -a "$NEW_VERSION" -m "release $NEW_VERSION"
echo "release: version=$NEW_VERSION committed and tagged"
