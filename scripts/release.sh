#!/usr/bin/env bash
set -euo pipefail

ENV_FILE="${ENV_FILE:-.env}"

if [[ ! -f "$ENV_FILE" ]]; then
  echo "release: missing $ENV_FILE"
  exit 1
fi

get_current_version() {
  grep -E '^version=' "$ENV_FILE" | head -1 | cut -d= -f2-
}

if [[ -n "${V:-}" ]]; then
  NEW_VERSION="${V//$'\r'/}"
else
  CUR="$(get_current_version)"
  CUR="${CUR//$'\r'/}"
  CUR="${CUR#"${CUR%%[![:space:]]*}"}"
  CUR="${CUR%"${CUR##*[![:space:]]}"}"

  if [[ "$CUR" =~ ^(v)?([0-9]+)\.([0-9]+)\.([0-9]+)$ ]]; then
    prefix="${BASH_REMATCH[1]}"
    major="${BASH_REMATCH[2]}"
    minor="${BASH_REMATCH[3]}"
    patch="${BASH_REMATCH[4]}"
    patch=$((patch + 1))
    NEW_VERSION="${prefix}${major}.${minor}.${patch}"
  else
    echo "release: cannot bump non-semver version=${CUR:-<empty>} (use: make release V=v0.0.1)"
    exit 1
  fi
fi

replace_version_inplace() {
  local file="$1" val="$2"
  if sed --version >/dev/null 2>&1; then
    sed -i "s/^version=.*/version=${val}/" "$file"
  else
    sed -i '' "s/^version=.*/version=${val}/" "$file"
  fi
}

if git rev-parse "$NEW_VERSION" >/dev/null 2>&1; then
  echo "release: git tag already exists: $NEW_VERSION"
  exit 1
fi

replace_version_inplace "$ENV_FILE" "$NEW_VERSION"
git tag -a "$NEW_VERSION" -m "release ${NEW_VERSION}"
echo "release: version=${NEW_VERSION} updated in ${ENV_FILE}, tag ${NEW_VERSION} created"
