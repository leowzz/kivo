# Release Firmware Assets Design

## Goal

Publish ready-to-flash ESP32-S3 and RP2040 firmware beside the macOS and
Windows installers in every tagged GitHub Release, then document the shortest
supported flashing path for each controller.

App-integrated firmware download or flashing is outside this change. The Kivo
desktop application and its device-management UI remain unchanged.

## Release Assets

For a tag such as `v0.4.2`, the release workflow publishes these additional
assets:

- `kivo-v0.4.2-esp32s3.bin`
- `kivo-v0.4.2-rp2040.uf2`

Both firmware builds receive the complete Git tag as
`KIVO_FIRMWARE_BUILD_ID`, so the runtime `HELLO` response identifies the exact
release that produced the image.

The RP2040 asset is PlatformIO's existing `firmware.uf2`. The ESP32-S3 asset is
a single factory image that can be written at flash address `0x0`. It combines
the images used by the existing PlatformIO upload operation:

| Address | Image |
|---|---|
| `0x0000` | ESP32-S3 bootloader |
| `0x8000` | partition table |
| `0xe000` | Arduino boot application metadata |
| `0x10000` | Kivo application firmware |

A PlatformIO post-build script invokes the installed `esptool` package to
create this merged image. This keeps the image layout coupled to the build
environment and makes the same artifact available from local ESP32-S3 builds
and CI without adding another tool dependency.

## Workflow Architecture

The existing desktop release matrix continues to build and publish the macOS
universal DMG and Windows x64 NSIS installer.

A Linux firmware-build job starts in parallel with that matrix. It installs
the locked Python/PlatformIO environment, builds both firmware environments,
renames the two user-facing files with the current tag, and uploads them as a
short-lived GitHub Actions artifact.

A firmware-publish job depends on both the desktop release matrix and the
firmware-build job. It downloads the temporary artifact and uploads both files
to the already-created GitHub Release. This ordering avoids a race between
Release creation and firmware upload without serializing the expensive build
work.

If either desktop publishing or firmware building fails, the final firmware
upload does not run. Re-running a successful publication uses overwrite-safe
asset upload so a partially completed release can be repaired without changing
the tag.

The release description is updated to mention both desktop installers and
firmware images.

## README Experience

A user-facing firmware section is placed near Quick Start, before development
instructions. It links to the latest GitHub Release and maps each supported
board to the correct asset.

YD-RP2040 uses the native drag-and-drop path:

1. Enter BOOTSEL mode by either holding BOOT while connecting USB, or, while
   already connected, holding BOOT, tapping RESET, and then releasing BOOT.
   Wait for the `RPI-RP2` drive.
2. Drag the release `.uf2` file onto that drive.
3. Wait for the drive to disappear and the controller to restart.

LuatOS ESP32-S3-AIO uses Espressif's browser flasher because its ROM download
mode does not expose a file-manager drive:

1. Use Chrome or Edge and enter download mode by holding BOOT, tapping RESET,
   and then releasing BOOT.
2. Open the official ESP Tool, connect the board, and add the release `.bin`
   at address `0x0`.
3. Select `Program`, then reset the controller when flashing completes.

The existing `make upload-esp32s3` and `make upload-rp2040` commands remain in
the developer firmware section as repeatable, hardware-identity-aware upload
paths.

## Failure Handling

Firmware packaging fails immediately when an expected PlatformIO output is
missing or `esptool` cannot merge the ESP32-S3 images. GitHub Release upload
fails if the tag's release does not exist after the desktop jobs complete.

The README explicitly distinguishes the two bootloader experiences so users do
not wait for a nonexistent ESP32-S3 file-manager drive or try to program the
RP2040 UF2 through the ESP browser tool.

## Verification

Automated release checks assert that:

- both PlatformIO environments are built with the release tag as build ID;
- the expected versioned asset names are staged;
- firmware publication waits for both desktop publishing and firmware build;
- both assets are uploaded to the current tag;
- README instructions retain the RP2040 `RPI-RP2` drag-and-drop path and the
  ESP32-S3 `0x0` browser-flashing path.

Executable verification builds both firmware environments. The resulting
ESP32-S3 merged image is checked against its input images at the documented
offsets, and the RP2040 UF2 output is checked for existence and nonzero size.
