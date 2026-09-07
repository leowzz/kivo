#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

npm run build:studio >/dev/null
test -f dist-studio/studio.html
test ! -f dist-studio/index.html
grep -Fq 'assets/' dist-studio/studio.html
! grep -Fq '/src/studio/main.tsx' dist-studio/studio.html
grep -Fq '/src/studio/main.tsx' studio.html
# Studio must never bundle the main app's device/action runtime bridge.
! grep -RqE 'save_runtime_assignment|save_device_profile|runtime-event' dist-studio/assets/*.js
grep -q 'studio_read_gpio' dist-studio/assets/*.js
node --input-type=module -e '
import { readFileSync } from "node:fs";
import assert from "node:assert/strict";
const config = JSON.parse(readFileSync("src-tauri/tauri.studio.conf.json", "utf8"));
assert.equal(config.app.windows[0].url, "studio.html");
assert.equal(config.build.frontendDist, "../dist-studio");
assert.deepEqual(config.bundle.resources, []);
'
