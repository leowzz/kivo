# Task 8 Report: Bounded SSD1306 Dirty-Tile Refresh

## Scope

Implemented from base `e848671` with changes limited to the Task 8 scheduler,
RP2040 display driver, native tests, and this ignored task report. The existing
ESP32-S3 `serviceDisplay()` no-op and the existing `main.cpp` call after input
scanning and before the 1 ms delay already satisfied the approved integration
contract, so neither required a source change.

## RED

Added native tests before creating or changing production scheduler code. The
tests cover:

- the required 128-byte right-half/two-row counter region;
- a 64-byte maximum run and row-local coalescing;
- outward 8x8 rounding, panel clipping, and row boundaries;
- no dequeue below one 8-byte tile, explicit queue clearing, and the 512-byte
  full-screen total;
- full-refresh fallback for unsupported partial updates or non-zero rotation.

Command:

```text
rtk uv run pio test -e native
```

Expected RED, exit 1:

```text
test/test_gpio_trigger/test_main.cpp:10:10: fatal error: 'DirtyTiles.h' file not found
10 | #include "DirtyTiles.h"
   |          ^~~~~~~~~~~~~~
1 error generated.
*** [.pio/build/native/test/test_gpio_trigger/test_main.o] Error 1
```

The failure was the intended missing scheduler API, not a test typo or an
unrelated regression.

## GREEN

- Added a fixed-capacity `DirtyTiles` scheduler backed by one 64-bit bitmap.
- `markPixels` rounds outward to 8x8 tiles, clips to the configured 16x4 panel,
  and ORs new dirty regions into unsent work.
- `takeRun` scans row-major, coalesces adjacent bits only within one row, caps
  the run at `maxDataBytes / 8`, and clears only the returned bits.
- RP2040 remote full/delta renders now update the full U8g2 framebuffer and
  enqueue all/dirty tiles without synchronously calling `sendBuffer()`.
- `serviceDisplay()` dequeues at most one 64-byte tile run per loop and calls
  `updateDisplayArea(tx, ty, tw, th)`.
- The fallback mode performs one full `sendBuffer()` on the first service call
  and clears the pending bitmap. The current SSD1306 full-buffer/R0 driver
  selects tile mode.
- Local status remains an immediate full redraw. Local render, remote reset,
  reconfiguration stop, and power-down clear pending remote work first.
- Task 7 precondition failures still return `false` before framebuffer mutation
  and therefore retain the existing local `DISPLAY ERROR` escalation path.

First GREEN command:

```text
rtk uv run pio test -e native
```

Result: 89/89 native Unity test cases passed. The same 89/89 result remained
GREEN after removing an unnecessary `markAll` wrapper in favor of marking the
full 128x32 pixel bounds through the tested API.

## U8g2 Semantics Check

Inspected installed U8g2 2.36.18 source. Its `u8g2_UpdateDisplayArea` comments
define positions and sizes as pixel coordinates divided by 8, require a full
buffer, ignore rotation, and require U8x8 display support. The implementation
offsets by `tx * 8`, calls `u8x8_DrawTile(..., tw, ...)` once per `th` row, and
advances by one framebuffer page per row. This confirms the scheduler payload
calculation `tw * 8 * th` and the rotation/capability fallback.

## Verification Evidence

- `rtk uv run pio test -e native`: PASS, 89/89.
- `rtk test bash test/test_release.sh`: PASS.
- `rtk direnv exec . make build-rp2040`: PASS; RAM 22,912/262,144 bytes
  (8.7%), flash 133,176/16,773,120 bytes (0.8%). The build compiled
  `DirtyTiles.cpp` and linked the U8g2 partial-update call.
- `rtk direnv exec . make build-esp32s3`: PASS; RAM 35,644/327,680 bytes
  (10.9%), flash 348,169/3,342,336 bytes (10.4%). The platform display service
  remains a no-op.
- `rtk git diff --check`: PASS.

These commands are rerun fresh immediately before the Task 8 commit; the final
commit hash is reported to the parent task separately.

## Self-Review

- Byte-budget math: one 8x8 monochrome tile is 8 payload bytes; the 64-byte
  budget caps a run at eight tiles, and height is always one tile row.
- Stale-buffer race: serial commits run before the one post-scan service call.
  New commits mutate the same full framebuffer and OR dirty bits, so service
  reads the latest bytes instead of deliberately transmitting an old snapshot.
- Main-loop ordering: `readHelperResponses`, runtime/learning scan,
  `platform::serviceDisplay`, then `platform::delayMs(1)` remains unchanged.
- Failure paths: unsupported operations still fail before mutation; stop/reset
  clear the queue; fallback clears only after its single full send; local
  critical/startup frames bypass the remote queue and send immediately.

## Concerns

- No physical firmware upload, GPIO scan-latency measurement, or OLED visual
  inspection was requested or performed. Automated tests and both firmware
  builds are evidence of logic/link correctness, not physical acceptance of
  the 64-byte timing budget.
- U8g2 `updateDisplayArea()` and `sendBuffer()` return `void`. As in Task 7,
  allocation and `begin()` failures are detectable, but post-begin I2C transfer
  failure cannot be escalated from this API.
