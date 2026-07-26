# Dual LED Toggle Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build and flash ESP32-S3 firmware that blinks LEDA at 3 Hz, keeps LEDB on, and swaps those roles once per debounced GPIO6 low pulse.

**Architecture:** A hardware-independent `LedController` owns blink timing, input debounce, and role state. A small Arduino entry point reads GPIO6, calls the controller with `esp_timer_get_time()`, and writes the returned levels to GPIO10 and GPIO11. PlatformIO runs Unity tests natively on macOS and builds/uploads the ESP32-S3 firmware.

**Tech Stack:** C++17, PlatformIO, Arduino-ESP32, Unity, ESP32-S3 `esp_timer`

---

## File Map

- `.gitignore`: excludes the local Python environment and PlatformIO outputs.
- `platformio.ini`: defines the native test and ESP32-S3 firmware environments.
- `lib/led_controller/src/LedController.h`: public controller API, constants, and board pin mapping.
- `lib/led_controller/src/LedController.cpp`: drift-resistant blink and debounced role-switch state machine.
- `test/test_led_controller/test_main.cpp`: native behavioral tests for all controller requirements.
- `src/main.cpp`: Arduino GPIO adapter and firmware loop.

### Task 1: Create the Reproducible PlatformIO Skeleton

**Files:**
- Create: `.gitignore`
- Create: `platformio.ini`

- [ ] **Step 1: Create the local tool environment**

Run:

```bash
/opt/homebrew/bin/python3.13 -m venv .venv
./.venv/bin/python -m pip install --upgrade pip
./.venv/bin/pip install platformio
```

Expected: `./.venv/bin/pio --version` prints `PlatformIO Core` and exits 0.

- [ ] **Step 2: Add generated-file exclusions**

Create `.gitignore`:

```gitignore
.pio/
.venv/
.DS_Store
```

- [ ] **Step 3: Configure native tests and the ESP32-S3 target**

Create `platformio.ini`:

```ini
[platformio]
default_envs = esp32s3

[env]
lib_ldf_mode = deep+

[env:native]
platform = native
test_framework = unity
build_flags = -std=c++17

[env:esp32s3]
platform = espressif32
board = esp32-s3-devkitc-1
framework = arduino
build_flags = -std=gnu++17
upload_port = /dev/cu.usbmodem575E0212961
monitor_port = /dev/cu.usbmodem575E0212961
monitor_speed = 115200
```

- [ ] **Step 4: Validate PlatformIO configuration**

Run:

```bash
./.venv/bin/pio project config
```

Expected: both `[env:native]` and `[env:esp32s3]` are listed without configuration errors.

- [ ] **Step 5: Commit the skeleton**

```bash
git add .gitignore platformio.ini
git commit -m "build: configure PlatformIO for ESP32-S3"
```

### Task 2: Implement the Startup State and 3 Hz Cadence

**Files:**
- Create: `test/test_led_controller/test_main.cpp`
- Create: `lib/led_controller/src/LedController.h`
- Create: `lib/led_controller/src/LedController.cpp`

- [ ] **Step 1: Write failing startup and cadence tests**

Create `test/test_led_controller/test_main.cpp`:

```cpp
#include <unity.h>

#include "LedController.h"

void setUp() {}
void tearDown() {}

void test_starts_with_led_a_on_and_led_b_steady_on() {
  LedController controller(0);

  const LedOutputs outputs = controller.update(0, true);

  TEST_ASSERT_TRUE(outputs.ledA);
  TEST_ASSERT_TRUE(outputs.ledB);
}

void test_led_a_completes_three_flashes_per_second_without_drift() {
  LedController controller(0);

  TEST_ASSERT_TRUE(controller.update(166666, true).ledA);
  TEST_ASSERT_FALSE(controller.update(166667, true).ledA);
  TEST_ASSERT_TRUE(controller.update(333334, true).ledA);
  TEST_ASSERT_FALSE(controller.update(833335, true).ledA);
  TEST_ASSERT_TRUE(controller.update(1000002, true).ledA);
  TEST_ASSERT_TRUE(controller.update(1000002, true).ledB);
}

int main(int, char **) {
  UNITY_BEGIN();
  RUN_TEST(test_starts_with_led_a_on_and_led_b_steady_on);
  RUN_TEST(test_led_a_completes_three_flashes_per_second_without_drift);
  return UNITY_END();
}
```

- [ ] **Step 2: Run the test and verify RED**

Run:

```bash
./.venv/bin/pio test -e native
```

Expected: compilation fails because `LedController.h` does not exist. This is the expected missing-feature failure.

- [ ] **Step 3: Add the minimal timing controller**

Create `lib/led_controller/src/LedController.h`:

```cpp
#pragma once

#include <cstdint>

struct LedOutputs {
  bool ledA;
  bool ledB;
};

class LedController {
 public:
  static constexpr std::uint64_t kHalfPeriodUs = 166667;
  static constexpr std::uint64_t kDebounceUs = 30000;

  explicit LedController(std::uint64_t startUs = 0);

  void reset(std::uint64_t startUs);
  LedOutputs update(std::uint64_t nowUs, bool inputHigh);

 private:
  bool blinkLedA_ = true;
  bool blinkOn_ = true;
  std::uint64_t nextBlinkToggleUs_ = kHalfPeriodUs;
};
```

Create `lib/led_controller/src/LedController.cpp`:

```cpp
#include "LedController.h"

LedController::LedController(std::uint64_t startUs) { reset(startUs); }

void LedController::reset(std::uint64_t startUs) {
  blinkLedA_ = true;
  blinkOn_ = true;
  nextBlinkToggleUs_ = startUs + kHalfPeriodUs;
}

LedOutputs LedController::update(std::uint64_t nowUs, bool) {
  if (nowUs >= nextBlinkToggleUs_) {
    const std::uint64_t intervals =
        ((nowUs - nextBlinkToggleUs_) / kHalfPeriodUs) + 1;
    if ((intervals & 1U) != 0U) {
      blinkOn_ = !blinkOn_;
    }
    nextBlinkToggleUs_ += intervals * kHalfPeriodUs;
  }

  return blinkLedA_ ? LedOutputs{blinkOn_, true}
                    : LedOutputs{true, blinkOn_};
}
```

- [ ] **Step 4: Run the tests and verify GREEN**

Run:

```bash
./.venv/bin/pio test -e native
```

Expected: 2 tests pass with no failures.

- [ ] **Step 5: Commit the cadence behavior**

```bash
git add lib/led_controller test/test_led_controller
git commit -m "feat: add drift-resistant 3 Hz LED cadence"
```

### Task 3: Add Debounced, Repeatable Role Switching

**Files:**
- Modify: `test/test_led_controller/test_main.cpp`
- Modify: `lib/led_controller/src/LedController.h`
- Modify: `lib/led_controller/src/LedController.cpp`

- [ ] **Step 1: Add failing debounce and held-low tests**

Add these test functions before `main`:

```cpp
void test_stable_low_swaps_roles_once_after_debounce() {
  LedController controller(0);

  controller.update(1000, false);
  TEST_ASSERT_TRUE(controller.update(30999, false).ledA);

  const LedOutputs switched = controller.update(31000, false);
  TEST_ASSERT_TRUE(switched.ledA);
  TEST_ASSERT_TRUE(switched.ledB);

  const LedOutputs held = controller.update(231000, false);
  TEST_ASSERT_TRUE(held.ledA);
  TEST_ASSERT_FALSE(held.ledB);
}

void test_bounce_does_not_switch_and_stable_release_rearms_input() {
  LedController controller(0);

  controller.update(1000, false);
  controller.update(10000, true);
  controller.update(15000, false);
  controller.update(20000, true);
  TEST_ASSERT_TRUE(controller.update(50000, true).ledA);

  controller.update(60000, false);
  TEST_ASSERT_TRUE(controller.update(90000, false).ledA);

  controller.update(100000, true);
  controller.update(130000, true);
  controller.update(140000, false);
  const LedOutputs switchedBack = controller.update(170000, false);

  TEST_ASSERT_TRUE(switchedBack.ledA);
  TEST_ASSERT_TRUE(switchedBack.ledB);
  TEST_ASSERT_FALSE(controller.update(336667, false).ledA);
  TEST_ASSERT_TRUE(controller.update(336667, false).ledB);
}
```

Add these registrations inside `main`:

```cpp
RUN_TEST(test_stable_low_swaps_roles_once_after_debounce);
RUN_TEST(test_bounce_does_not_switch_and_stable_release_rearms_input);
```

- [ ] **Step 2: Run the tests and verify RED**

Run:

```bash
./.venv/bin/pio test -e native
```

Expected: the two new tests fail because GPIO6 input is ignored and LEDA remains the blinking LED.

- [ ] **Step 3: Add debounce state to the controller**

Add these members to the private section of `LedController`:

```cpp
bool rawInputHigh_ = true;
bool stableInputHigh_ = true;
bool inputArmed_ = true;
std::uint64_t rawInputChangedUs_ = 0;

void updateInput(std::uint64_t nowUs, bool inputHigh);
```

Add these assignments to `reset`:

```cpp
rawInputHigh_ = true;
stableInputHigh_ = true;
inputArmed_ = true;
rawInputChangedUs_ = startUs;
```

Add the input state machine to `LedController.cpp`:

```cpp
void LedController::updateInput(std::uint64_t nowUs, bool inputHigh) {
  if (inputHigh != rawInputHigh_) {
    rawInputHigh_ = inputHigh;
    rawInputChangedUs_ = nowUs;
  }

  if (rawInputHigh_ == stableInputHigh_ ||
      nowUs - rawInputChangedUs_ < kDebounceUs) {
    return;
  }

  stableInputHigh_ = rawInputHigh_;
  if (stableInputHigh_) {
    inputArmed_ = true;
    return;
  }

  if (inputArmed_) {
    blinkLedA_ = !blinkLedA_;
    blinkOn_ = true;
    nextBlinkToggleUs_ = nowUs + kHalfPeriodUs;
    inputArmed_ = false;
  }
}
```

Change the start of `update` so it processes the sampled input:

```cpp
LedOutputs LedController::update(std::uint64_t nowUs, bool inputHigh) {
  updateInput(nowUs, inputHigh);
```

Keep the existing timing and output logic below that call unchanged.

- [ ] **Step 4: Run the tests and verify GREEN**

Run:

```bash
./.venv/bin/pio test -e native
```

Expected: all 4 tests pass.

- [ ] **Step 5: Commit role switching**

```bash
git add lib/led_controller test/test_led_controller/test_main.cpp
git commit -m "feat: switch LED roles on debounced GPIO6 pulses"
```

### Task 4: Connect the Controller to the ESP32-S3 Pins

**Files:**
- Modify: `test/test_led_controller/test_main.cpp`
- Modify: `lib/led_controller/src/LedController.h`
- Create: `src/main.cpp`

- [ ] **Step 1: Write a failing board-mapping test**

Add this function before `main`:

```cpp
void test_uses_luatos_esp32s3_aio_board_pins() {
  TEST_ASSERT_EQUAL_UINT8(10, BoardPins::kLedA);
  TEST_ASSERT_EQUAL_UINT8(11, BoardPins::kLedB);
  TEST_ASSERT_EQUAL_UINT8(6, BoardPins::kModeInput);
}
```

Register it inside `main`:

```cpp
RUN_TEST(test_uses_luatos_esp32s3_aio_board_pins);
```

- [ ] **Step 2: Run the test and verify RED**

Run:

```bash
./.venv/bin/pio test -e native
```

Expected: compilation fails because `BoardPins` is not defined.

- [ ] **Step 3: Add the board mapping**

Add this declaration above `LedOutputs` in `LedController.h`:

```cpp
struct BoardPins {
  static constexpr std::uint8_t kLedA = 10;
  static constexpr std::uint8_t kLedB = 11;
  static constexpr std::uint8_t kModeInput = 6;
};
```

- [ ] **Step 4: Run the native tests and verify GREEN**

Run:

```bash
./.venv/bin/pio test -e native
```

Expected: all 5 tests pass.

- [ ] **Step 5: Add the Arduino hardware adapter**

Create `src/main.cpp`:

```cpp
#include <Arduino.h>
#include <esp_timer.h>

#include "LedController.h"

namespace {
LedController controller;
}

void setup() {
  pinMode(BoardPins::kLedA, OUTPUT);
  pinMode(BoardPins::kLedB, OUTPUT);
  pinMode(BoardPins::kModeInput, INPUT_PULLUP);

  const auto nowUs = static_cast<std::uint64_t>(esp_timer_get_time());
  controller.reset(nowUs);
  const LedOutputs outputs = controller.update(nowUs, digitalRead(BoardPins::kModeInput) == HIGH);
  digitalWrite(BoardPins::kLedA, outputs.ledA ? HIGH : LOW);
  digitalWrite(BoardPins::kLedB, outputs.ledB ? HIGH : LOW);
}

void loop() {
  const auto nowUs = static_cast<std::uint64_t>(esp_timer_get_time());
  const bool inputHigh = digitalRead(BoardPins::kModeInput) == HIGH;
  const LedOutputs outputs = controller.update(nowUs, inputHigh);

  digitalWrite(BoardPins::kLedA, outputs.ledA ? HIGH : LOW);
  digitalWrite(BoardPins::kLedB, outputs.ledB ? HIGH : LOW);
}
```

- [ ] **Step 6: Build the ESP32-S3 firmware**

Run:

```bash
./.venv/bin/pio run -e esp32s3
```

Expected: `SUCCESS` and `.pio/build/esp32s3/firmware.bin` exists.

- [ ] **Step 7: Commit the hardware adapter**

```bash
git add lib/led_controller/src/LedController.h test/test_led_controller/test_main.cpp src/main.cpp
git commit -m "feat: connect LED controller to LuatOS board pins"
```

### Task 5: Upload and Verify the Complete Firmware

**Files:**
- No source changes expected

- [ ] **Step 1: Confirm the board serial port is present**

Run:

```bash
test -c /dev/cu.usbmodem575E0212961
```

Expected: exit status 0.

- [ ] **Step 2: Run the complete automated verification**

Run:

```bash
./.venv/bin/pio test -e native
./.venv/bin/pio run -e esp32s3
```

Expected: all 5 tests pass and the firmware build reports `SUCCESS`.

- [ ] **Step 3: Upload through the CH343 serial bridge**

Run:

```bash
./.venv/bin/pio run -e esp32s3 -t upload
```

Expected: PlatformIO detects ESP32-S3, writes and verifies the firmware, then resets the board.

- [ ] **Step 4: Perform the physical behavior check**

Observe LEDA blinking three complete times per second while LEDB stays on. Briefly connect GPIO6 to GND, release it, and confirm LEDA becomes steady while LEDB blinks. Repeat the low pulse and confirm the original roles return; holding GPIO6 low must not cause repeated swaps.

- [ ] **Step 5: Record final repository state**

Run:

```bash
git status --short --branch
git log --oneline --decorate -5
```

Expected: the worktree is clean and the implementation commits appear above the design and build commits.
