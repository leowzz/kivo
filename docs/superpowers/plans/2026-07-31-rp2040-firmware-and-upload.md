# RP2040 Firmware And Upload Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Produce protocol-v3 ESP32-S3 and VCC-GND YD-RP2040 firmware targets that share scanning/action logic, preserve USB HID execution, and upload only to an explicitly selected hardware serial.

**Architecture:** Shared C++ owns topology, learning, ordered action acknowledgement, and protocol formatting. Compile-time platform adapters own CDC, HID, USB descriptors, and the concrete `BoardProfile`; PlatformIO builds exactly one adapter per environment.

**Tech Stack:** C++17, PlatformIO, Unity native tests, Arduino-ESP32, Arduino-Pico at pinned platform commit `aa70b802be8851668053d4f09734e4089fe41932`, Adafruit TinyUSB, Python/pyserial, picotool.

## Global Constraints

- Runtime protocol is exactly v3; `HELLO 2` is not accepted or emitted.
- `HELLO` is `HELLO 3 <family> <board> <build> <pin-count> <pins...>`.
- ESP32-S3 remains `303a:4002`, family `esp32s3`, board `luatos-esp32s3-aio`.
- YD-RP2040 runtime is `2e8a:102e`, family `rp2040`, board `vccgnd-yd-rp2040`; ROM boot is `2e8a:0003`.
- YD-RP2040 exposes exactly GPIO0-22; GPIO23-29 are rejected in topology and learning.
- Both targets retain CDC plus keyboard HID; the serial-only host-input design is excluded.
- Every build, test, and Git command is prefixed with `rtk`.

---

### Task 1: Inject Board-Specific Pin Safety Into Shared Firmware

**Files:**
- Create: `lib/gpio_trigger/src/BoardProfile.h`
- Modify: `lib/gpio_trigger/src/InputTopology.h`
- Modify: `lib/gpio_trigger/src/InputTopology.cpp`
- Modify: `lib/gpio_trigger/src/GpioTriggerController.h`
- Modify: `lib/gpio_trigger/src/GpioTriggerController.cpp`
- Test: `test/test_gpio_trigger/test_main.cpp`

**Interfaces:**
- Consumes: Existing `RuntimeTopology`, `TopologyBuilder`, and `GpioTriggerController` behavior.
- Produces: `BoardProfile::supports(uint8_t)`, `kLuatOsEsp32S3Aio`, `kVccGndYdRp2040`, `TopologyBuilder(const BoardProfile&)`, and `GpioTriggerController(const BoardProfile&, uint32_t startMs = 0)`.

- [ ] **Step 1: Write failing board-boundary tests**

Add these assertions and update test constructors to pass a Board Profile:

```cpp
void test_board_profiles_enforce_exact_safe_pins() {
  TEST_ASSERT_TRUE(kLuatOsEsp32S3Aio.supports(18));
  TEST_ASSERT_FALSE(kLuatOsEsp32S3Aio.supports(19));
  TEST_ASSERT_TRUE(kVccGndYdRp2040.supports(0));
  TEST_ASSERT_TRUE(kVccGndYdRp2040.supports(22));
  for (std::uint8_t pin = 23; pin <= 29; ++pin) {
    TEST_ASSERT_FALSE(kVccGndYdRp2040.supports(pin));
  }

  TopologyBuilder rp2040(kVccGndYdRp2040);
  TEST_ASSERT_TRUE(rp2040.begin(1, 30));
  TEST_ASSERT_TRUE(rp2040.addDirect(1, 0, {0, 22}));

  TopologyBuilder reserved(kVccGndYdRp2040);
  TEST_ASSERT_TRUE(reserved.begin(1, 30));
  TEST_ASSERT_FALSE(reserved.addDirect(1, 0, {23}));
}
```

Register `test_board_profiles_enforce_exact_safe_pins` in `main()`.

- [ ] **Step 2: Run the native test and verify the new symbols are missing**

Run: `rtk test uv run pio test -e native`

Expected: FAIL compiling because `BoardProfile`, `kLuatOsEsp32S3Aio`, and `kVccGndYdRp2040` do not exist.

- [ ] **Step 3: Add the immutable Board Profile contract**

Create `BoardProfile.h`:

```cpp
#pragma once

#include <array>
#include <cstddef>
#include <cstdint>

struct BoardProfile {
  const char *controllerFamilyId;
  const char *boardProfileId;
  const std::uint8_t *safePins;
  std::size_t safePinCount;

  bool supports(std::uint8_t pin) const {
    for (std::size_t index = 0; index < safePinCount; ++index) {
      if (safePins[index] == pin) return true;
    }
    return false;
  }
};

inline constexpr std::array<std::uint8_t, 17> kEsp32S3SafePins = {
    0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 12, 13, 14, 15, 16, 17, 18};
inline constexpr std::array<std::uint8_t, 23> kYdRp2040SafePins = {
    0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11,
    12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22};

inline constexpr BoardProfile kLuatOsEsp32S3Aio = {
    "esp32s3", "luatos-esp32s3-aio", kEsp32S3SafePins.data(),
    kEsp32S3SafePins.size()};
inline constexpr BoardProfile kVccGndYdRp2040 = {
    "rp2040", "vccgnd-yd-rp2040", kYdRp2040SafePins.data(),
    kYdRp2040SafePins.size()};
```

Change `TopologyBuilder` and `GpioTriggerController` to store `const BoardProfile& profile_`; replace every read of `kEsp32S3SafePins` or static `kSupportedPins` with `profile_.supports(pin)` and indexed iteration over `safePins[0..safePinCount)`. Give the controller's `startMs` parameter its existing `= 0` default. Update existing tests to construct ESP32-S3 instances explicitly:

```cpp
TopologyBuilder builder(kLuatOsEsp32S3Aio);
GpioTriggerController controller(kLuatOsEsp32S3Aio, 0);
```

- [ ] **Step 4: Run the complete native firmware suite**

Run: `rtk test uv run pio test -e native`

Expected: PASS, including the exact ESP32-S3 and GPIO0-22 RP2040 boundaries.

- [ ] **Step 5: Commit the board-boundary change**

```bash
rtk git add lib/gpio_trigger/src/BoardProfile.h lib/gpio_trigger/src/InputTopology.h lib/gpio_trigger/src/InputTopology.cpp lib/gpio_trigger/src/GpioTriggerController.h lib/gpio_trigger/src/GpioTriggerController.cpp test/test_gpio_trigger/test_main.cpp
rtk git commit -m "refactor: make firmware topology board-aware"
```

---

### Task 2: Move USB And GPIO Operations Behind Compile-Time Adapters

**Files:**
- Create: `src/platform/Platform.h`
- Create: `src/platform/esp32s3.cpp`
- Modify: `src/main.cpp`
- Modify: `platformio.ini`
- Test: `test/test_gpio_trigger/test_main.cpp`

**Interfaces:**
- Consumes: `BoardProfile` and existing ESP32-S3 CDC/HID behavior.
- Produces: Free-function platform API `platform::boardProfile`, `begin`, `connected`, `available`, `read`, `write`, `flush`, `sendHotkey`, and `delayMs`.

- [ ] **Step 1: Run the existing ESP32-S3 build before refactoring**

Run: `rtk proxy uv run pio run -e esp32s3`

Expected: PASS and produce `.pio/build/esp32s3/firmware.bin`.

- [ ] **Step 2: Define the platform interface and move ESP32-S3 code**

Create `src/platform/Platform.h`:

```cpp
#pragma once

#include <cstddef>
#include <cstdint>

#include "BoardProfile.h"

namespace platform {
const BoardProfile &boardProfile();
void begin();
bool connected();
int available();
int read();
void write(const char *data, std::size_t size);
void flush();
void sendHotkey(std::uint8_t modifiers, std::uint8_t keycode);
void delayMs(std::uint32_t milliseconds);
}  // namespace platform
```

Move `USB`, `USBCDC`, and `USBHIDKeyboard` objects into `esp32s3.cpp`. Implement `sendHotkey` with the existing `KeyReport`, 10 ms press duration, and unconditional `releaseAll()`. `boardProfile()` returns `kLuatOsEsp32S3Aio`; `begin()` preserves VID/PID `303a:4002`, manufacturer `Kivo`, and product `Kivo Keyboard`.

Change shared `main.cpp` to construct:

```cpp
GpioTriggerController controller(platform::boardProfile());
TopologyBuilder topologyBuilder(platform::boardProfile());
```

Replace direct CDC/HID operations with the platform functions. When resetting pin modes, loop from index `0` to `safePinCount - 1` and read `safePins[index]`; the profile stores a pointer plus count and is not itself a range.

- [ ] **Step 3: Select exactly one adapter per firmware environment**

Add these filters:

```ini
[env:esp32s3]
build_src_filter = +<*> -<platform/rp2040.cpp>

[env:native]
build_src_filter = -<main.cpp> -<platform/*>
```

- [ ] **Step 4: Rebuild native and ESP32-S3 targets**

Run: `rtk test uv run pio test -e native`

Expected: PASS.

Run: `rtk proxy uv run pio run -e esp32s3`

Expected: PASS with no RP2040 headers compiled into the ESP32-S3 target.

- [ ] **Step 5: Commit the platform extraction**

```bash
rtk git add src/main.cpp src/platform/Platform.h src/platform/esp32s3.cpp platformio.ini test/test_gpio_trigger/test_main.cpp
rtk git commit -m "refactor: isolate ESP32-S3 firmware platform code"
```

---

### Task 3: Emit The Strict Protocol-v3 Handshake

**Files:**
- Create: `lib/gpio_trigger/src/Handshake.h`
- Create: `lib/gpio_trigger/src/Handshake.cpp`
- Create: `scripts/platformio_build_id.py`
- Modify: `src/main.cpp`
- Modify: `platformio.ini`
- Test: `test/test_gpio_trigger/test_main.cpp`

**Interfaces:**
- Consumes: `BoardProfile` and `KIVO_FIRMWARE_BUILD_ID`.
- Produces: `formatHello(const BoardProfile&, std::string_view) -> std::string`.

- [ ] **Step 1: Write failing HELLO format tests**

```cpp
void test_formats_protocol_v3_hello_with_board_and_build() {
  TEST_ASSERT_EQUAL_STRING(
      "HELLO 3 rp2040 vccgnd-yd-rp2040 0.1.0+gabc1234 23 "
      "0 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 19 20 21 22\n",
      formatHello(kVccGndYdRp2040, "0.1.0+gabc1234").c_str());
}
```

Add cases rejecting an empty build ID and a build ID containing whitespace by returning an empty string.

- [ ] **Step 2: Run the native test and verify the formatter is missing**

Run: `rtk test uv run pio test -e native`

Expected: FAIL compiling because `formatHello` does not exist.

- [ ] **Step 3: Implement the formatter and remove the hard-coded v2 line**

Declare:

```cpp
std::string formatHello(const BoardProfile &profile,
                        std::string_view firmwareBuildId);
```

Build the line from `controllerFamilyId`, `boardProfileId`, the exact safe-pin count, and pins in declared order. Return an empty string when the build token is empty or contains ASCII whitespace. In `main.cpp` construct the line once:

```cpp
const std::string helloLine =
    formatHello(platform::boardProfile(), KIVO_FIRMWARE_BUILD_ID);
```

Use it both after CDC connection and for the `HELLO` command. Do not retain a v2 response.

- [ ] **Step 4: Add one validated build-ID injection path**

Create `scripts/platformio_build_id.py` so every firmware environment receives the same quoted token without replacing its existing compiler flags:

```python
import os
import re

Import("env")  # type: ignore[name-defined]  # PlatformIO provides this symbol.

build_id = os.environ.get("KIVO_FIRMWARE_BUILD_ID", "0.1.0+dev")
if not re.fullmatch(r"\S+", build_id):
    raise ValueError("KIVO_FIRMWARE_BUILD_ID must be one non-whitespace token")
env.Append(CPPDEFINES=[("KIVO_FIRMWARE_BUILD_ID", env.StringifyMacro(build_id))])
```

Register it once in the common environment:

```ini
[env]
lib_ldf_mode = deep+
extra_scripts = pre:scripts/platformio_build_id.py
```

Release and upload commands set `KIVO_FIRMWARE_BUILD_ID=<version>` in the process environment. The checked-in default remains deterministic for development builds.

- [ ] **Step 5: Run native and ESP32-S3 verification**

Run: `rtk test uv run pio test -e native`

Expected: PASS with exact v3 output and malformed-build rejection.

Run: `rtk proxy env KIVO_FIRMWARE_BUILD_ID=0.1.0+test uv run pio run -e esp32s3`

Expected: PASS. Then run `rtk proxy strings .pio/build/esp32s3/firmware.bin | rtk proxy rg "HELLO 2"`; expected: exit 1 with no match.

- [ ] **Step 6: Commit protocol v3 firmware output**

```bash
rtk git add lib/gpio_trigger/src/Handshake.h lib/gpio_trigger/src/Handshake.cpp scripts/platformio_build_id.py src/main.cpp platformio.ini test/test_gpio_trigger/test_main.cpp
rtk git commit -m "feat: emit board-aware protocol v3 handshakes"
```

---

### Task 4: Add The YD-RP2040 TinyUSB Adapter

**Files:**
- Create: `src/platform/rp2040.cpp`
- Modify: `platformio.ini`
- Modify: `src/main.cpp`

**Interfaces:**
- Consumes: `platform/Platform.h`, Arduino-Pico `Serial`, `TinyUSBDevice`, and `Adafruit_USBD_HID`.
- Produces: CDC plus keyboard HID runtime USB device `2e8a:102e` using the ROM-stable hardware serial and `kVccGndYdRp2040`.

- [ ] **Step 1: Add the pinned RP2040 environment**

```ini
[env:rp2040]
platform = https://github.com/maxgerhardt/platform-raspberrypi.git#aa70b802be8851668053d4f09734e4089fe41932
board = vccgnd_yd_rp2040
framework = arduino
board_build.core = earlephilhower
board_build.usb_stack = tinyusb
upload_protocol = picotool
build_src_filter = +<*> -<platform/esp32s3.cpp>
build_unflags =
  -std=gnu++11
build_flags =
  -std=gnu++17
  -DUSE_TINYUSB
```

- [ ] **Step 2: Run the RP2040 build and verify the adapter is missing**

Run: `rtk proxy uv run pio run -e rp2040`

Expected: FAIL linking the `platform::*` functions.

- [ ] **Step 3: Implement TinyUSB CDC and keyboard HID**

Use this adapter shape:

```cpp
#include <Adafruit_TinyUSB.h>
#include <Arduino.h>

#include "Platform.h"

namespace {
std::uint8_t const kKeyboardDescriptor[] = {TUD_HID_REPORT_DESC_KEYBOARD()};
Adafruit_USBD_HID keyboard(kKeyboardDescriptor, sizeof(kKeyboardDescriptor),
                           HID_ITF_PROTOCOL_KEYBOARD, 2, false);
}

namespace platform {
const BoardProfile &boardProfile() { return kVccGndYdRp2040; }

void begin() {
  TinyUSBDevice.setID(0x2e8a, 0x102e);
  TinyUSBDevice.setManufacturerDescriptor("VCC-GND");
  TinyUSBDevice.setProductDescriptor("Kivo Keyboard RP2040");
  Serial.begin(115200);
  keyboard.begin();
}

bool connected() { return static_cast<bool>(Serial); }
int available() { return Serial.available(); }
int read() { return Serial.read(); }
void write(const char *data, std::size_t size) {
  Serial.write(reinterpret_cast<const std::uint8_t *>(data), size);
}
void flush() { Serial.flush(); }

void sendHotkey(std::uint8_t modifiers, std::uint8_t keycode) {
  hid_keyboard_report_t report{};
  report.modifier = modifiers;
  report.keycode[0] = keycode;
  keyboard.sendReport(0, &report, sizeof(report));
  delay(10);
  hid_keyboard_report_t released{};
  keyboard.sendReport(0, &released, sizeof(released));
}

void delayMs(std::uint32_t milliseconds) { delay(milliseconds); }
}  // namespace platform
```

Do not set a synthetic constant serial descriptor. Arduino-Pico/TinyUSB must use its hardware-derived serial; the physical check in Task 6 proves it matches ROM Boot serial.

- [ ] **Step 4: Build all firmware environments**

Run: `rtk test uv run pio test -e native`

Expected: PASS.

Run: `rtk proxy uv run pio run -e esp32s3`

Expected: PASS.

Run: `rtk proxy uv run pio run -e rp2040`

Expected: PASS and produce `.pio/build/rp2040/firmware.uf2`.

- [ ] **Step 5: Commit the RP2040 target**

```bash
rtk git add src/platform/rp2040.cpp platformio.ini src/main.cpp
rtk git commit -m "feat: add YD-RP2040 CDC and HID firmware"
```

---

### Task 5: Require Explicit Serial Targets For Upload

**Files:**
- Modify: `scripts/enter_download_mode.py`
- Create: `scripts/list_firmware_targets.py`
- Create: `scripts/smoke_runtime_protocol.py`
- Create: `scripts/verify_runtime_firmware.py`
- Modify: `Makefile`
- Test: `test/test_upload_targeting.py`
- Test: `test/test_runtime_smoke.py`
- Modify: `pyproject.toml`

**Interfaces:**
- Consumes: pyserial port metadata, PlatformIO upload-port selection, and picotool `--ser` selection.
- Produces: `make upload-esp32s3 SERIAL=<serial>` and `make upload-rp2040 SERIAL=<serial>`; neither command auto-selects a device.

- [ ] **Step 1: Write failing Python target-selection tests**

Extract pure selection functions and test exact matching:

```python
def test_select_port_requires_exact_serial() -> None:
    ports = [
        FakePort("/dev/a", 0x303A, 0x4002, "AAA"),
        FakePort("/dev/b", 0x303A, 0x4002, "BBB"),
    ]
    assert select_runtime_port(ports, (0x303A, 0x4002), "BBB").device == "/dev/b"
    with pytest.raises(TargetError, match="serial CCC not found"):
        select_runtime_port(ports, (0x303A, 0x4002), "CCC")

def test_missing_serial_argument_is_rejected() -> None:
    with pytest.raises(TargetError, match="SERIAL is required"):
        require_serial("")
```

- [ ] **Step 2: Run the tests and verify the targeting API is missing**

Run: `rtk test uv run pytest test/test_upload_targeting.py`

Expected: FAIL importing `select_runtime_port`, `TargetError`, and `require_serial`.

- [ ] **Step 3: Make ESP32-S3 download mode serial-specific**

Add `argparse --serial`, require a non-empty value, filter the runtime `303a:4002` ports by `port.serial_number`, and fail unless exactly one matching port exists. Capture that port's pyserial `location`, trigger only it at 1200 baud, then wait up to 10 seconds for exactly one `303a:1001` port at the same USB location. If download mode exposes a serial, also require it to match; do not require a serial when that boot ROM omits it. Print only the resolved download-mode port path to stdout and send diagnostics to stderr. USB location is transient continuity for this one upload and is never stored as Device ID. Add tests for two same-board Devices at different locations and ambiguous/missing location. Remove every branch that accepts the first or only Kivo port.

- [ ] **Step 4: Add runtime handshake verification**

Implement `verify_runtime_firmware.py` arguments `--serial`, `--vid`, `--pid`, `--family`, `--board`, and `--build`. Wait up to 10 seconds for the exact serial, open at 115200, send `HELLO\n`, and require tokens `["HELLO", "3", family, board, build]`; fail on any mismatch.

Create `list_firmware_targets.py` as a read-only inventory command. Print one tab-separated row for every recognized runtime or bootloader candidate: mode, VID:PID, Board Profile, serial, and port or `-` when the bootloader has no CDC port. Use pyserial for CDC devices and parse the structured JSON from `system_profiler SPUSBDataType -json` on macOS for UF2 candidates; never choose or upload a row in this script.

Create `smoke_runtime_protocol.py` with required `--serial`, `--vid`, `--pid`, `--family`, `--board`, `--valid-pins`, and `--rejected-pins` arguments plus optional `--exercise-actions`. It opens only the exact runtime identity, verifies HELLO v3, sends a valid direct topology and requires `CONFIG_OK`, sends one topology per rejected pin and requires `CONFIG_ERROR ... invalid_direct`, then begins/ends learning on the valid pins. With `--exercise-actions`, it waits for a physical `STATE ... DOWN`, replies with `PASTE` then `HOTKEY` for that exact event, and requires matching sequential `DONE` lines. Unit-test the command/response state machine with a fake serial transport before physical use.

- [ ] **Step 5: Add explicit Make targets**

```make
.PHONY: build-esp32s3 build-rp2040 upload-esp32s3 upload-rp2040

BUILD_ID ?= 0.1.0+dev

require-serial:
	@test -n "$(SERIAL)" || { echo "SERIAL is required" >&2; exit 2; }

build-esp32s3:
	KIVO_FIRMWARE_BUILD_ID="$(BUILD_ID)" uv run pio run -e esp32s3

build-rp2040:
	KIVO_FIRMWARE_BUILD_ID="$(BUILD_ID)" uv run pio run -e rp2040

upload-esp32s3: require-serial build-esp32s3
	@download_port="$$(uv run python scripts/enter_download_mode.py --serial "$(SERIAL)")"; \
	  KIVO_FIRMWARE_BUILD_ID="$(BUILD_ID)" uv run pio run -e esp32s3 -t upload --upload-port "$$download_port"
	uv run python scripts/verify_runtime_firmware.py --serial "$(SERIAL)" --vid 0x303a --pid 0x4002 --family esp32s3 --board luatos-esp32s3-aio --build "$(BUILD_ID)"

upload-rp2040: require-serial build-rp2040
	uv run pio pkg exec -p tool-picotool-rp2040-earlephilhower -- picotool load -x .pio/build/rp2040/firmware.uf2 --ser "$(SERIAL)"
	uv run python scripts/verify_runtime_firmware.py --serial "$(SERIAL)" --vid 0x2e8a --pid 0x102e --family rp2040 --board vccgnd-yd-rp2040 --build "$(BUILD_ID)"
```

Define `BUILD_ID ?= 0.1.0+dev` and pass the same value through `KIVO_FIRMWARE_BUILD_ID` for both builds. This preserves all compiler and USB flags declared in `platformio.ini`. Add `"pyserial>=3.5,<4"` and `"pytest>=8,<9"` to `[dependency-groups].dev`, then run `rtk proxy uv lock` so scripts and tests use direct reproducible dependencies.

- [ ] **Step 6: Run targeting tests and command guards**

Run: `rtk test uv run pytest test/test_upload_targeting.py test/test_runtime_smoke.py`

Expected: PASS.

Run: `rtk proxy make upload-rp2040`

Expected: exit 2 with `SERIAL is required` before any build or write.

Run: `rtk proxy make upload-esp32s3 SERIAL=DOES_NOT_EXIST BUILD_ID=0.1.0+dev`

Expected: fail with an exact serial-not-found error and never select another connected device.

- [ ] **Step 7: Commit explicit upload targeting**

```bash
rtk git add scripts/enter_download_mode.py scripts/list_firmware_targets.py scripts/smoke_runtime_protocol.py scripts/verify_runtime_firmware.py Makefile test/test_upload_targeting.py test/test_runtime_smoke.py pyproject.toml uv.lock
rtk git commit -m "feat: target firmware uploads by hardware serial"
```

---

### Task 6: Verify Both Physical Firmware Targets

**Files:**
- Create: `docs/verification/2026-07-31-rp2040-firmware-evidence.md`

**Interfaces:**
- Consumes: Completed firmware artifacts and explicit upload commands.
- Produces: Recorded physical evidence for descriptors, v3 handshake, GPIO boundary, CDC, HID, and coexistence.

- [ ] **Step 1: Run all non-destructive firmware checks**

Run: `rtk test uv run pio test -e native`

Expected: PASS.

Run: `rtk proxy uv run pio run -e esp32s3`

Expected: PASS.

Run: `rtk proxy uv run pio run -e rp2040`

Expected: PASS and `.pio/build/rp2040/firmware.uf2` exists.

- [ ] **Step 2: Flash the explicitly observed RP2040**

Run: `rtk proxy make upload-rp2040 SERIAL=E0C9125B0D9B BUILD_ID=0.1.0+dev`

Expected: picotool selects only serial `E0C9125B0D9B`; runtime returns as `2e8a:102e` with the same serial and exact v3 build ID.

- [ ] **Step 3: Perform the RP2040 smoke test**

Run `rtk proxy uv run python scripts/smoke_runtime_protocol.py --serial E0C9125B0D9B --vid 0x2e8a --pid 0x102e --family rp2040 --board vccgnd-yd-rp2040 --valid-pins 0,22 --rejected-pins 23,29 --exercise-actions`. Confirm the valid topology reaches CONFIG_OK, both rejected pins reach CONFIG_ERROR, learning reports the physically exercised direct/contact event, Paste emits GUI+V HID, Hotkey emits the requested report, and every action returns matching DONE.

- [ ] **Step 4: Rebuild and smoke-test an explicitly selected ESP32-S3**

Run `rtk proxy uv run python scripts/list_firmware_targets.py`, choose the serial printed on the `303a:4002 luatos-esp32s3-aio` row, and pass that exact value to `rtk proxy make upload-esp32s3 SERIAL=<printed-serial> BUILD_ID=0.1.0+dev`. The operator-supplied `SERIAL` is required; no command derives or auto-selects it.

Expected: runtime remains `303a:4002`, reports `HELLO 3 esp32s3 luatos-esp32s3-aio 0.1.0+dev ...`, and existing paste/hotkey timing remains functional.

- [ ] **Step 5: Record evidence and commit**

Create the evidence document with one table row per tested physical Device. Record exact observed VID/PID, serial, HELLO line, GPIO boundary result, CDC result, HID result, command timestamp, and pass/fail outcome. Link the approved design without editing its normative text.

```bash
rtk git add docs/verification/2026-07-31-rp2040-firmware-evidence.md
rtk git commit -m "test: record RP2040 and ESP32-S3 firmware evidence"
```
