#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT

MAKEFILE="$ROOT/Makefile"
FIRMWARE_MAIN="$ROOT/src/main.cpp"
PLATFORM_HEADER="$ROOT/src/platform/Platform.h"
RP2040_PLATFORM="$ROOT/src/platform/rp2040.cpp"
ESP32S3_PLATFORM="$ROOT/src/platform/esp32s3.cpp"
DISPLAY_CONTROLLER="$ROOT/lib/gpio_trigger/src/DisplayController.h"
PLATFORMIO_CONFIG="$ROOT/platformio.ini"
ESP32S3_MERGE_SCRIPT="$ROOT/scripts/merge_esp32s3_firmware.py"
RELEASE_WORKFLOW="$ROOT/.github/workflows/release-windows.yml"
WINDOWS_WORKFLOW="$ROOT/.github/workflows/windows-ci.yml"
README="$ROOT/README.md"

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
grep -Fq 'python scripts/repo_version.py check "${GITHUB_REF_NAME}"' \
  "$RELEASE_WORKFLOW"
! grep -Fq 'config.version = tag.slice(1)' "$RELEASE_WORKFLOW"
release_job_body="$(awk '
  /^  release:$/ { capture = 1; next }
  /^  firmware-publish:$/ { exit }
  capture { print }
' "$RELEASE_WORKFLOW")"
grep -Fq 'uses: actions/setup-python@v6' <<<"$release_job_body"
copy_env_line="$(grep -n -F 'cp .env.example .env' <<<"$release_job_body" | cut -d: -f1)"
check_version_line="$(grep -n -F 'python scripts/repo_version.py check "${GITHUB_REF_NAME}"' \
  <<<"$release_job_body" | cut -d: -f1)"
test -n "$copy_env_line"
test -n "$check_version_line"
test "$copy_env_line" -lt "$check_version_line"
grep -Fq 'Copy-Item .env.example .env' "$WINDOWS_WORKFLOW"
grep -Fq 'cp .env.example .env' "$README"
grep -Fq 'Copy-Item .env.example .env' "$README"
grep -Fq 'version=vX.Y.Z' "$README"
grep -Fq 'test/test_repo_version.py' "$MAKEFILE"
grep -Fq 'test/test_release_transaction.py' "$MAKEFILE"
grep -Fq 'test/test_platformio_build_id.py' "$MAKEFILE"
grep -Fq '## 刷入固件' "$README"
grep -Fq 'kivo-vX.Y.Z-esp32s3.bin' "$README"
grep -Fq 'kivo-vX.Y.Z-rp2040.uf2' "$README"
grep -Fq 'RPI-RP2' "$README"
grep -Fq '按住 **BOOT**，插入 USB' "$README"
grep -Fq '按住 **BOOT**，短按一次 **RESET**，然后松开 **BOOT**' "$README"
grep -Fq 'https://espressif.github.io/esptool-js/' "$README"
grep -Fq '地址填写 `0x0`' "$README"
grep -Fq '点击 **Program**' "$README"

grep -Fq 'bool configureDisplay(const std::optional<OledConfig> &config);' \
  "$PLATFORM_HEADER"
grep -Fq 'bool renderLocalDisplay(const DisplayFrame &frame);' "$PLATFORM_HEADER"
grep -Fq 'bool renderRemoteDisplay(const RemoteDisplayCommit &scene,' "$PLATFORM_HEADER"
grep -Fq 'void resetRemoteDisplay();' "$PLATFORM_HEADER"
grep -Fq 'void serviceDisplay();' "$PLATFORM_HEADER"
grep -Fq 'bool configureDisplay(const std::optional<OledConfig> &config)' \
  "$RP2040_PLATFORM"
grep -Fq 'new (std::nothrow)' "$RP2040_PLATFORM"
grep -Fq 'if (!display->begin())' "$RP2040_PLATFORM"
grep -Fq 'bool renderLocalDisplay(const DisplayFrame &frame)' "$RP2040_PLATFORM"
grep -Fq 'bool renderRemoteDisplay(const RemoteDisplayCommit &scene,' "$RP2040_PLATFORM"
grep -Fq 'operation.fontId > kRemoteDisplayMaxFontId' "$RP2040_PLATFORM"
grep -Fq 'display->setFont(u8g2_font_6x13_tf);' "$RP2040_PLATFORM"
grep -Fq 'u8g2_font_9x18_tf' "$RP2040_PLATFORM"
grep -Fq 'u8g2_font_10x20_tf' "$RP2040_PLATFORM"
grep -Fq 'display->drawBox(bounds.x, bounds.y, bounds.width, bounds.height);' \
  "$RP2040_PLATFORM"
grep -Fq 'display->sendBuffer();' "$RP2040_PLATFORM"
grep -Fq 'bool configureDisplay(const std::optional<OledConfig> &config)' \
  "$ESP32S3_PLATFORM"
grep -Fq 'return !config.has_value();' "$ESP32S3_PLATFORM"
grep -Fq 'bool renderRemoteDisplay(const RemoteDisplayCommit &, bool)' \
  "$ESP32S3_PLATFORM"
grep -Fq 'DisplayUpdate commitRemote(const RemoteDisplayCommit &scene);' \
  "$DISPLAY_CONTROLLER"
grep -Fq 'DisplayUpdate helperConnected(const DisplayFrame &ready);' \
  "$DISPLAY_CONTROLLER"
grep -Fq 'DisplayController displayController;' "$FIRMWARE_MAIN"
grep -Fq 'platform::renderLocalDisplay(*update.local);' "$FIRMWARE_MAIN"
grep -Fq 'platform::renderRemoteDisplay(*update.remote, update.fullRedraw);' \
  "$FIRMWARE_MAIN"
grep -Fq 'platform::resetRemoteDisplay();' "$FIRMWARE_MAIN"
grep -Fq 'displayController.displayReconfigured()' "$FIRMWARE_MAIN"
grep -Fq 'displayController.displayFailed(displayFailureFrame())' \
  "$FIRMWARE_MAIN"
grep -Fq 'displayController.helperConnected(displayStatus.frame())' \
  "$FIRMWARE_MAIN"
grep -Fq 'responseLines = ResponseLineBuffer(kMaxResponseLineLength);' \
  "$FIRMWARE_MAIN"
grep -Fq 'if (helperConnected) readHelperResponses(nowMs);' "$FIRMWARE_MAIN"
grep -Fq 'remoteDisplay.emplace();' "$FIRMWARE_MAIN"
! grep -Fq 'remoteDisplay = RemoteDisplay{};' "$FIRMWARE_MAIN"
grep -Fq 'platform::serviceDisplay();' "$FIRMWARE_MAIN"
! grep -Fq 'renderDisplay(' "$FIRMWARE_MAIN"
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
grep -Fq 'displayController.clearLocalOverride()' <<<"$activate_topology_body"
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
grep -Fq 'expected = ["HELLO", "8", family, board, build]' "$ROOT/scripts/verify_runtime_firmware.py"

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
  '$(UV_CMD) run pytest test/test_repo_version.py test/test_release_transaction.py test/test_platformio_build_id.py'
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

FRESH_REPO="$TMP_DIR/fresh-checkout"
mkdir -p "$FRESH_REPO/scripts" "$FRESH_REPO/src-tauri"
for version_file in \
  .env.example \
  package.json \
  package-lock.json \
  pyproject.toml \
  uv.lock \
  src-tauri/Cargo.toml \
  src-tauri/Cargo.lock \
  src-tauri/tauri.conf.json; do
  cp "$ROOT/$version_file" "$FRESH_REPO/$version_file"
done
cp "$ROOT/scripts/repo_version.py" "$FRESH_REPO/scripts/repo_version.py"
cp "$FRESH_REPO/.env.example" "$FRESH_REPO/.env"
(cd "$FRESH_REPO" && python scripts/repo_version.py set v1.2.3)
rm "$FRESH_REPO/.env"
expected_tag="$(awk '
  /^version=/ {
    count += 1
    value = substr($0, length("version=") + 1)
    next
  }
  { invalid = 1 }
  END {
    if (count != 1 || invalid || value == "") exit 1
    print value
  }
' "$FRESH_REPO/.env.example")"

if missing_env_output="$(cd "$FRESH_REPO" && python scripts/repo_version.py check "$expected_tag" 2>&1)"; then
  echo "fresh checkout unexpectedly passed without .env" >&2
  exit 1
fi
grep -Fq 'missing ' <<<"$missing_env_output"
grep -Fq '.env' <<<"$missing_env_output"
grep -Fq 'cp .env.example .env' <<<"$missing_env_output"
cp "$FRESH_REPO/.env.example" "$FRESH_REPO/.env"
(cd "$FRESH_REPO" && python scripts/repo_version.py check "$expected_tag")
