#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

npm run build -- --mode studio >/dev/null
test -f dist-studio/index.html
grep -Fq 'assets/' dist-studio/index.html
! grep -Fq '/src/studio/main.tsx' dist-studio/index.html
grep -Fq '/src/studio/main.tsx' studio.html
