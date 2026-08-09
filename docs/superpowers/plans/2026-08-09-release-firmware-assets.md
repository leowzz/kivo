# Release Firmware Assets Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Publish versioned ESP32-S3 and RP2040 firmware files in every tagged GitHub Release and document exact, low-friction flashing steps for both boards.

**Architecture:** PlatformIO creates a merged ESP32-S3 factory image after the normal application image, while RP2040 keeps its existing UF2 output. A Linux job builds both firmwares in parallel with the desktop release matrix, stages the versioned files as a temporary Actions artifact, and a dependent job uploads them after the GitHub Release exists.

**Tech Stack:** GitHub Actions, PlatformIO 6, Python/SCons build hooks, esptool, Bash release checks, Markdown.

---

## File Map

- Create `scripts/merge_esp32s3_firmware.py`: PlatformIO post-build hook that merges the ESP32-S3 flash images into `firmware.factory.bin`.
- Modify `platformio.ini`: load the ESP32-S3 merge hook after each PlatformIO environment is configured; the hook activates only for `esp32s3`.
- Modify `.github/workflows/release-windows.yml`: build, stage, and publish both firmware assets alongside desktop installers.
- Modify `test/test_release.sh`: cover merge-hook registration, workflow ordering and assets, and user-facing flashing instructions.
- Modify `README.md`: add release asset selection, exact BOOT/RESET sequences, RP2040 drag-and-drop, ESP32-S3 browser flashing, and retain developer upload commands.

### Task 1: ESP32-S3 Factory Image

**Files:**
- Create: `scripts/merge_esp32s3_firmware.py`
- Modify: `platformio.ini`
- Modify: `test/test_release.sh`

- [ ] **Step 1: Write failing build-hook checks**

After the existing path variables near the top of `test/test_release.sh`, add:

```bash
PLATFORMIO_CONFIG="$ROOT/platformio.ini"
ESP32S3_MERGE_SCRIPT="$ROOT/scripts/merge_esp32s3_firmware.py"

grep -Fq 'post:scripts/merge_esp32s3_firmware.py' "$PLATFORMIO_CONFIG"
test -f "$ESP32S3_MERGE_SCRIPT"
grep -Fq 'env.get("FLASH_EXTRA_IMAGES", [])' "$ESP32S3_MERGE_SCRIPT"
grep -Fq 'env.subst("$ESP32_APP_OFFSET")' "$ESP32S3_MERGE_SCRIPT"
grep -Fq '"merge_bin"' "$ESP32S3_MERGE_SCRIPT"
grep -Fq '.factory.bin' "$ESP32S3_MERGE_SCRIPT"
```

- [ ] **Step 2: Run RED**

Run:

```bash
bash test/test_release.sh
```

Expected: FAIL because `platformio.ini` does not register the post-build hook.

- [ ] **Step 3: Implement the PlatformIO merge hook**

Change the shared `extra_scripts` list in `platformio.ini` to:

```ini
extra_scripts =
  pre:scripts/platformio_build_id.py
  post:scripts/merge_esp32s3_firmware.py
```

Create `scripts/merge_esp32s3_firmware.py`:

```python
from pathlib import Path
import subprocess

Import("env")


def merge_factory_image(source, target, env):
    build_dir = Path(env.subst("$BUILD_DIR"))
    program_name = env.subst("$PROGNAME")
    firmware = build_dir / f"{program_name}.bin"
    output = build_dir / f"{program_name}.factory.bin"
    board = env.BoardConfig()
    images = [
        (offset, Path(env.subst(path)))
        for offset, path in env.get("FLASH_EXTRA_IMAGES", [])
    ]
    images.append((env.subst("$ESP32_APP_OFFSET"), firmware))

    command = [
        env.subst("$PYTHONEXE"),
        env.subst("$OBJCOPY"),
        "--chip",
        board.get("build.mcu"),
        "merge_bin",
        "--flash_mode",
        "keep",
        "--flash_freq",
        "keep",
        "--flash_size",
        "keep",
        "-o",
        str(output),
    ]
    for offset, path in images:
        command.extend((str(offset), str(path)))

    subprocess.run(command, check=True)


if env.subst("$PIOENV") == "esp32s3":
    env.AddPostAction(
        "$BUILD_DIR/${PROGNAME}.bin",
        env.VerboseAction(
            merge_factory_image,
            "Building $BUILD_DIR/${PROGNAME}.factory.bin",
        ),
    )
```

- [ ] **Step 4: Run GREEN**

Run:

```bash
bash test/test_release.sh
```

Expected: PASS.

- [ ] **Step 5: Prove the hook creates a nonempty merged image**

Run:

```bash
KIVO_FIRMWARE_BUILD_ID=v0.0.0-plan-check uv run pio run -e esp32s3
test -s .pio/build/esp32s3/firmware.factory.bin
```

Expected: PlatformIO reports `SUCCESS`, logs `Building .../firmware.factory.bin`, and the size check exits zero.

- [ ] **Step 6: Commit the factory-image change**

```bash
git add platformio.ini scripts/merge_esp32s3_firmware.py test/test_release.sh
git commit -m "build: create ESP32-S3 factory image"
```

### Task 2: Parallel Firmware Release Jobs

**Files:**
- Modify: `.github/workflows/release-windows.yml`
- Modify: `test/test_release.sh`

- [ ] **Step 1: Write failing workflow checks**

Add `RELEASE_WORKFLOW="$ROOT/.github/workflows/release-windows.yml"` with the other paths in `test/test_release.sh`, then add:

```bash
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
```

- [ ] **Step 2: Run RED**

Run:

```bash
bash test/test_release.sh
```

Expected: FAIL because the workflow has no `firmware-build` job.

- [ ] **Step 3: Add the parallel firmware build job**

Add this job under `jobs:` in `.github/workflows/release-windows.yml`, alongside the existing `release` job:

```yaml
  firmware-build:
    name: Build release firmware
    runs-on: ubuntu-latest

    steps:
      - name: Checkout
        uses: actions/checkout@v7

      - name: Set up Python and uv
        uses: astral-sh/setup-uv@v7
        with:
          enable-cache: true
          python-version: "3.13"

      - name: Install firmware dependencies
        run: uv sync --locked

      - name: Build firmware
        run: |
          KIVO_FIRMWARE_BUILD_ID="${GITHUB_REF_NAME}" uv run pio run -e esp32s3
          KIVO_FIRMWARE_BUILD_ID="${GITHUB_REF_NAME}" uv run pio run -e rp2040

      - name: Stage release firmware
        run: |
          mkdir release-firmware
          cp .pio/build/esp32s3/firmware.factory.bin "release-firmware/kivo-${GITHUB_REF_NAME}-esp32s3.bin"
          cp .pio/build/rp2040/firmware.uf2 "release-firmware/kivo-${GITHUB_REF_NAME}-rp2040.uf2"

      - name: Upload staged firmware
        uses: actions/upload-artifact@v6
        with:
          name: release-firmware
          path: release-firmware/
          if-no-files-found: error
          retention-days: 1
```

- [ ] **Step 4: Add the ordered publication job**

Add this job after the existing desktop `release` job:

```yaml
  firmware-publish:
    name: Publish release firmware
    needs: [release, firmware-build]
    runs-on: ubuntu-latest

    steps:
      - name: Download staged firmware
        uses: actions/download-artifact@v7
        with:
          name: release-firmware
          path: release-firmware

      - name: Upload firmware to GitHub Release
        env:
          GH_TOKEN: ${{ secrets.GITHUB_TOKEN }}
        run: >-
          gh release upload "${GITHUB_REF_NAME}" release-firmware/* --clobber
          --repo "${GITHUB_REPOSITORY}"
```

Change the existing Tauri action release body to:

```yaml
          releaseBody: "macOS universal DMG, Windows x64 installer, and ESP32-S3 and RP2040 firmware."
```

- [ ] **Step 5: Run GREEN and parse the YAML**

Run:

```bash
bash test/test_release.sh
ruby -e 'require "yaml"; YAML.parse_file(".github/workflows/release-windows.yml")'
```

Expected: the release checks pass and Ruby exits zero without a YAML parse error.

- [ ] **Step 6: Commit the workflow change**

```bash
git add .github/workflows/release-windows.yml test/test_release.sh
git commit -m "ci: publish firmware with desktop releases"
```

### Task 3: End-User Flashing Instructions

**Files:**
- Modify: `README.md`
- Modify: `test/test_release.sh`

- [ ] **Step 1: Write failing README checks**

Add `README="$ROOT/README.md"` with the other paths in `test/test_release.sh`, then add:

```bash
grep -Fq '## 刷入固件' "$README"
grep -Fq 'kivo-vX.Y.Z-esp32s3.bin' "$README"
grep -Fq 'kivo-vX.Y.Z-rp2040.uf2' "$README"
grep -Fq 'RPI-RP2' "$README"
grep -Fq '按住 **BOOT**，插入 USB' "$README"
grep -Fq '按住 **BOOT**，短按一次 **RESET**，然后松开 **BOOT**' "$README"
grep -Fq 'https://espressif.github.io/esptool-js/' "$README"
grep -Fq '地址填写 `0x0`' "$README"
grep -Fq '点击 **Program**' "$README"
```

- [ ] **Step 2: Run RED**

Run:

```bash
bash test/test_release.sh
```

Expected: FAIL because the README has no end-user flashing section.

- [ ] **Step 3: Add the flashing section near Quick Start**

Add `刷入固件` to the top navigation links and insert the following section after the Quick Start note and before `配置怎样组合`:

```markdown
## 刷入固件

从 [最新 Release](https://github.com/leowzz/kivo/releases/latest) 下载与板卡对应的固件：

| 板卡 | 选择这个文件 |
|---|---|
| LuatOS ESP32-S3-AIO | `kivo-vX.Y.Z-esp32s3.bin` |
| VCC-GND YD-RP2040 | `kivo-vX.Y.Z-rp2040.uf2` |

### YD-RP2040：拖入文件管理器

1. 让板卡进入 BOOTSEL 模式：
   - 板卡尚未连接时，按住 **BOOT**，插入 USB；看到 `RPI-RP2` 磁盘后松开 **BOOT**。
   - 板卡已经连接时，按住 **BOOT**，短按一次 **RESET**，然后松开 **BOOT**。
2. 在 Finder 或文件资源管理器中打开 `RPI-RP2`。
3. 把 `kivo-vX.Y.Z-rp2040.uf2` 拖进磁盘。复制完成后磁盘会自动退出，板卡会运行 Kivo 固件。

### ESP32-S3：在浏览器中选择固件

ESP32-S3 的下载模式不会显示成磁盘。请使用 Chrome 或 Edge：

1. 下载 `kivo-vX.Y.Z-esp32s3.bin`，打开 Espressif 官方的 [ESP Tool](https://espressif.github.io/esptool-js/)。
2. 按住板卡的 **BOOT**，短按一次 **RESET/RST**，然后松开 **BOOT**。
3. 点击 **Connect**，选择刚出现的 ESP32-S3 串口。
4. 点击 **Add File**，地址填写 `0x0`，选择下载的 `.bin` 文件。
5. 点击 **Program**。完成后短按一次 **RESET/RST**，板卡会运行 Kivo 固件。

只使用上表中与板卡匹配的文件。刷写完成后保持 USB 连接，Kivo 会自动检测设备。
```

- [ ] **Step 4: Keep Quick Start consistent**

Change Quick Start step 2 from assuming pre-flashed hardware to directing new users to the new section:

```markdown
2. 按照[刷入固件](#刷入固件)为受支持的控制器刷入对应固件，然后连接控制器。通过身份与协议校验后，Kivo 会自动登记这台设备。
```

Keep the existing developer `make build-*` and `make upload-*` commands under `## 固件` unchanged.

- [ ] **Step 5: Run GREEN and check formatting**

Run:

```bash
bash test/test_release.sh
git diff --check -- README.md test/test_release.sh
```

Expected: both commands exit zero.

- [ ] **Step 6: Commit the README change**

```bash
git add README.md test/test_release.sh
git commit -m "docs: add release firmware flashing guide"
```

### Task 4: End-to-End Verification

**Files:**
- Verify: `.github/workflows/release-windows.yml`
- Verify: `platformio.ini`
- Verify: `scripts/merge_esp32s3_firmware.py`
- Verify: `README.md`
- Verify: `test/test_release.sh`

- [ ] **Step 1: Run the release checks and YAML parser**

```bash
bash test/test_release.sh
ruby -e 'require "yaml"; YAML.parse_file(".github/workflows/release-windows.yml")'
```

Expected: both commands exit zero.

- [ ] **Step 2: Build both release firmware formats from a clean build ID**

```bash
KIVO_FIRMWARE_BUILD_ID=v0.0.0-verification uv run pio run -e esp32s3
KIVO_FIRMWARE_BUILD_ID=v0.0.0-verification uv run pio run -e rp2040
test -s .pio/build/esp32s3/firmware.factory.bin
test -s .pio/build/rp2040/firmware.uf2
```

Expected: both PlatformIO environments report `SUCCESS` and both size checks exit zero.

- [ ] **Step 3: Verify every ESP32-S3 image at its release offset**

```bash
merged=.pio/build/esp32s3/firmware.factory.bin
boot_app="$(find "${PLATFORMIO_CORE_DIR:-$HOME/.platformio}/packages/framework-arduinoespressif32" -name boot_app0.bin -print -quit)"
test -n "$boot_app"
for entry in \
  "0:.pio/build/esp32s3/bootloader.bin" \
  "32768:.pio/build/esp32s3/partitions.bin" \
  "57344:$boot_app" \
  "65536:.pio/build/esp32s3/firmware.bin"
do
  offset="${entry%%:*}"
  image="${entry#*:}"
  size="$(wc -c < "$image" | tr -d ' ')"
  cmp -n "$size" -i "$offset" "$merged" "$image"
done
```

Expected: all four `cmp` calls exit zero.

- [ ] **Step 4: Run the repository regression suite**

```bash
make test
```

Expected: release/Python tests, native firmware tests, Rust tests and Clippy, frontend tests, and the production frontend build all pass.

- [ ] **Step 5: Inspect the final change set**

```bash
git status --short
git diff --check HEAD~3..HEAD
git log -4 --oneline
```

Expected: no uncommitted implementation changes, no whitespace errors, and the design plus three implementation commits are visible.
