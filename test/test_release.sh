#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT

MAKEFILE="$ROOT/Makefile"
FIRMWARE_MAIN="$ROOT/src/main.cpp"
RP2040_PLATFORM="$ROOT/src/platform/rp2040.cpp"
PLATFORMIO_CONFIG="$ROOT/platformio.ini"
ESP32S3_MERGE_SCRIPT="$ROOT/scripts/merge_esp32s3_firmware.py"
RELEASE_WORKFLOW="$ROOT/.github/workflows/release-windows.yml"

grep -Fq 'post:scripts/merge_esp32s3_firmware.py' "$PLATFORMIO_CONFIG"
test -f "$ESP32S3_MERGE_SCRIPT"
grep -Fq 'env.get("FLASH_EXTRA_IMAGES", [])' "$ESP32S3_MERGE_SCRIPT"
grep -Fq 'env.subst("$ESP32_APP_OFFSET")' "$ESP32S3_MERGE_SCRIPT"
grep -Fq '"merge_bin"' "$ESP32S3_MERGE_SCRIPT"
grep -Fq '.factory.bin' "$ESP32S3_MERGE_SCRIPT"
grep -Fq 'firmware-build:' "$RELEASE_WORKFLOW"
grep -Fq 'KIVO_FIRMWARE_BUILD_ID="${GITHUB_REF_NAME}" uv run pio run -e esp32s3' "$RELEASE_WORKFLOW"
grep -Fq 'KIVO_FIRMWARE_BUILD_ID="${GITHUB_REF_NAME}" uv run pio run -e rp2040' "$RELEASE_WORKFLOW"
grep -Fq 'kivo-${GITHUB_REF_NAME}-esp32s3.bin' "$RELEASE_WORKFLOW"
grep -Fq 'kivo-${GITHUB_REF_NAME}-rp2040.uf2' "$RELEASE_WORKFLOW"
grep -Fq 'name: release-firmware' "$RELEASE_WORKFLOW"
grep -Fq 'firmware-publish:' "$RELEASE_WORKFLOW"
grep -Fq 'needs: [release, firmware-build]' "$RELEASE_WORKFLOW"
grep -Fq 'gh release upload "${GITHUB_REF_NAME}" release-firmware/* --clobber' "$RELEASE_WORKFLOW"
grep -Fq 'ESP32-S3 and RP2040 firmware' "$RELEASE_WORKFLOW"

grep -Fq 'display->setFont(u8g2_font_6x13_tf);' "$RP2040_PLATFORM"
grep -Fq 'makeRp2040StandaloneDebugTopology(platform::boardProfile())' \
  "$FIRMWARE_MAIN"
grep -Fq 'initializeStandaloneDisplay(nowMs);' "$FIRMWARE_MAIN"
grep -Fq 'displayStatus.setStandaloneDebug(false);' "$FIRMWARE_MAIN"
! grep -Fq 'standalone_mismatch' "$FIRMWARE_MAIN"
activate_topology_body="$(awk '
  /^void activateTopology\(/ { capture = 1 }
  capture { print }
  capture && /^}/ { exit }
' "$FIRMWARE_MAIN")"
configure_display_line="$(grep -n 'platform::configureDisplay(topology.oled);' \
  <<<"$activate_topology_body" | cut -d: -f1)"
apply_topology_line="$(grep -n 'applyTopologyState(topology, nowMs);' \
  <<<"$activate_topology_body" | cut -d: -f1)"
test "$configure_display_line" -lt "$apply_topology_line"

target_body() {
  awk -v target="$1" '
    $0 ~ "^" target ":" { found = 1; next }
    found && /^[[:alnum:]_.-]+:/ { exit }
    found { print }
  ' "$MAKEFILE"
}

for target in build-esp32s3 build-rp2040 upload-esp32s3 upload-rp2040 require-serial; do
  grep -Eq "^${target}:" "$MAKEFILE"
done

require_serial_body="$(target_body require-serial)"
grep -Fq 'test -n "$(SERIAL)"' <<<"$require_serial_body"
grep -Fq 'expected = ["HELLO", "5", family, board, build]' "$ROOT/scripts/verify_runtime_firmware.py"

for target in upload-esp32s3 upload-rp2040; do
  ! grep -Eq "^${target}:[[:space:]].*require-serial([[:space:]]|$)" "$MAKEFILE"
  body="$(target_body "$target")"
  grep -Fq 'serial="$(SERIAL)"' <<<"$body"
  grep -Fq 'if [ -z "$$serial" ]' <<<"$body"
  grep -Fq 'scripts/select_firmware_target.py' <<<"$body"
  grep -Fq 'test -n "$$serial"' <<<"$body"
  grep -Fq 'scripts/verify_runtime_firmware.py' <<<"$body"
  grep -Fq -- '--serial "$$serial"' <<<"$body"
  grep -Fq -- '--build "$(BUILD_ID)"' <<<"$body"
done

esp32_upload_body="$(target_body upload-esp32s3)"
rp2040_upload_body="$(target_body upload-rp2040)"
test "$(grep -n -- '-t upload' <<<"$esp32_upload_body" | cut -d: -f1)" -lt "$(grep -n 'verify_runtime_firmware.py' <<<"$esp32_upload_body" | cut -d: -f1)"
grep -Fq -- 'esptool.py --chip esp32s3' <<<"$esp32_upload_body"
grep -Fq -- '--after hard_reset run' <<<"$esp32_upload_body"
grep -Fq -- '--board luatos-esp32s3-aio --mode runtime' <<<"$esp32_upload_body"
! grep -Fq -- '--mode bootloader' <<<"$esp32_upload_body"
test "$(grep -n 'select_firmware_target.py' <<<"$esp32_upload_body" | cut -d: -f1)" -lt "$(grep -n '$(ESP32S3_BUILD)' <<<"$esp32_upload_body" | cut -d: -f1)"
test "$(grep -n -- '-t upload' <<<"$esp32_upload_body" | cut -d: -f1)" -lt "$(grep -n -- 'esptool.py --chip esp32s3' <<<"$esp32_upload_body" | cut -d: -f1)"
test "$(grep -n -- 'esptool.py --chip esp32s3' <<<"$esp32_upload_body" | cut -d: -f1)" -lt "$(grep -n 'verify_runtime_firmware.py' <<<"$esp32_upload_body" | cut -d: -f1)"
grep -Fq -- '--board vccgnd-yd-rp2040 --mode runtime --mode bootloader' <<<"$rp2040_upload_body"
test "$(grep -n 'select_firmware_target.py' <<<"$rp2040_upload_body" | cut -d: -f1)" -lt "$(grep -n '$(RP2040_BUILD)' <<<"$rp2040_upload_body" | cut -d: -f1)"
grep -Fq 'scripts/upload_rp2040.py' <<<"$rp2040_upload_body"
grep -Fq -- '--firmware .pio/build/rp2040/firmware.uf2' <<<"$rp2040_upload_body"
grep -Fq 'runtime_serial=' <<<"$rp2040_upload_body"
test "$(grep -n -- 'scripts/upload_rp2040.py' <<<"$rp2040_upload_body" | cut -d: -f1)" -lt "$(grep -n 'verify_runtime_firmware.py' <<<"$rp2040_upload_body" | cut -d: -f1)"

grep -Eq '^upload:[[:space:]]*$' "$MAKEFILE"
upload_body="$(target_body upload)"
grep -Fq 'exit 2' <<<"$upload_body"
! grep -Fq 'SERIAL=' <<<"$upload_body"
! grep -Eq '(upload-esp32s3|upload-rp2040|enter_download_mode|picotool)' <<<"$upload_body"

test_body="$(target_body test)"
expected_test_commands=(
  'bash test/test_release.sh'
  '$(UV_CMD) run pytest test/test_upload_targeting.py test/test_rp2040_upload.py'
  '$(UV_CMD) run pytest test/test_firmware_target_selector.py test/test_make_upload_selection.py'
  '$(UV_CMD) run pio test -e native'
  'cargo test --manifest-path src-tauri/Cargo.toml'
  'cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings'
  'npm test'
  'npm run build'
)
previous_line=0
for command in "${expected_test_commands[@]}"; do
  line="$(grep -n -F "$command" <<<"$test_body" | cut -d: -f1)"
  test -n "$line"
  test "$line" -gt "$previous_line"
  previous_line="$line"
done
! grep -Eq -- '(^|[[:space:]])(upload|picotool|enter_download_mode)([[:space:]]|$)' <<<"$test_body"

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
