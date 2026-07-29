#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
BIN="$(mktemp -d)"
PIDS=()
trap 'kill "${PIDS[@]}" 2>/dev/null || true; rm -rf "$BIN"' EXIT

cat >"$BIN/pgrep" <<'EOF'
#!/usr/bin/env bash
[[ "${PGREP_OUTPUT:-}" ]] || exit 1
printf '%s\n' "$PGREP_OUTPUT"
EOF
chmod +x "$BIN/pgrep"

sleep 60 & PIDS+=("$!")
sleep 60 & PIDS+=("$!")
PGREP_OUTPUT="$(printf '%s\n' "${PIDS[@]}")" PATH="$BIN:$PATH" \
  make -C "$ROOT" --no-print-directory helper-kill
wait "${PIDS[0]}" 2>/dev/null || true
wait "${PIDS[1]}" 2>/dev/null || true
! kill -0 "${PIDS[0]}" 2>/dev/null
! kill -0 "${PIDS[1]}" 2>/dev/null

PATH="$BIN:$PATH" make -C "$ROOT" --no-print-directory helper-kill
