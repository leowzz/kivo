#include <Adafruit_NeoPixel.h>
#include <Adafruit_TinyUSB.h>
#include <Arduino.h>
#include <U8g2lib.h>
#include <Wire.h>

#include <array>
#include <memory>
#include <new>
#include <optional>

#include "HidReportTransport.h"
#include "DirtyTiles.h"
#include "Platform.h"
#include "Rp2040OledBus.h"

namespace {
constexpr std::uint8_t kKeyboardReportId = 1;
constexpr std::uint8_t kConsumerReportId = 2;
std::uint8_t const kKeyboardDescriptor[] = {
    TUD_HID_REPORT_DESC_KEYBOARD(HID_REPORT_ID(kKeyboardReportId)),
    TUD_HID_REPORT_DESC_CONSUMER(HID_REPORT_ID(kConsumerReportId)),
};
Adafruit_USBD_HID keyboard(kKeyboardDescriptor, sizeof(kKeyboardDescriptor),
                           HID_ITF_PROTOCOL_NONE, 2, false);
Adafruit_NeoPixel keyPixel(1, PIN_NEOPIXEL, NEO_GRB + NEO_KHZ800);
constexpr std::size_t kHidReadyPollLimit = 100;
constexpr std::uint8_t kKeyPixelBrightness = 64;
constexpr std::uint8_t kOledI2cAddress = 0x3C;
constexpr std::uint32_t kOledI2cClockHz = 100000;
constexpr std::uint8_t kDisplayWidth = 128;
constexpr std::uint8_t kDisplayWidthTiles = 16;
constexpr std::uint8_t kDisplayHeightTiles = 4;
constexpr std::size_t kDisplayServiceDataBytes = 64;
constexpr bool kPartialUpdateSupported = true;
constexpr std::uint16_t kDisplayRotationDegrees = 0;
constexpr std::array<std::uint8_t, 2> kStatusBaselines = {10, 29};
constexpr std::uint8_t kInputBaseline = 20;
std::unique_ptr<U8G2_SSD1306_128X32_UNIVISION_F_HW_I2C> i2c0Display;
std::unique_ptr<U8G2_SSD1306_128X32_UNIVISION_F_2ND_HW_I2C> i2c1Display;
std::unique_ptr<U8G2_SSD1306_128X32_UNIVISION_F_SW_I2C> softwareDisplay;
U8G2 *display = nullptr;
TwoWire *displayWire = nullptr;
std::optional<DisplayFrame> lastDisplayFrame;
enum class DisplayBufferSource { None, Local, Remote };
DisplayBufferSource displayBufferSource = DisplayBufferSource::None;
bool displayRequested = false;
bool displayHealthy = false;
DirtyTiles dirtyTiles(kDisplayWidthTiles, kDisplayHeightTiles);
RefreshMode refreshMode = RefreshMode::Full;

void stopDisplay() {
  dirtyTiles.clear();
  if (display && displayHealthy) {
    display->clearBuffer();
    display->sendBuffer();
    display->setPowerSave(1);
  }
  displayHealthy = false;
  display = nullptr;
  i2c0Display.reset();
  i2c1Display.reset();
  softwareDisplay.reset();
  if (displayWire) displayWire->end();
  displayWire = nullptr;
  lastDisplayFrame.reset();
  displayBufferSource = DisplayBufferSource::None;
}

void startHardwareI2c(TwoWire &wire, std::uint8_t sda, std::uint8_t scl) {
  wire.setSDA(sda);
  wire.setSCL(scl);
  wire.begin();
  wire.setClock(kOledI2cClockHz);
  displayWire = &wire;
}

bool supportsRemoteScene(const RemoteDisplayCommit &scene) {
  if (scene.regionCount > kMaxDisplayRegions ||
      scene.operationCount > kMaxDisplayOps ||
      scene.dirtyCount > kMaxDisplayRegions) {
    return false;
  }
  for (std::size_t index = 0; index < scene.operationCount; ++index) {
    const auto &operation = scene.operations[index];
    if (operation.kind == DisplayOperationKind::Clear) continue;
    if (operation.kind != DisplayOperationKind::Text ||
        operation.fontId != kRemoteDisplayFontId) {
      return false;
    }
  }
  return true;
}
}  // namespace

namespace platform {
const BoardProfile &boardProfile() { return kVccGndYdRp2040; }

void begin() {
  TinyUSBDevice.setID(0x2e8a, 0x102e);
  TinyUSBDevice.setManufacturerDescriptor("VCC-GND");
  TinyUSBDevice.setProductDescriptor("Kivo Keyboard RP2040");
  Serial.begin(115200);
  keyboard.begin();
  randomSeed(rp2040.hwrand32());
  keyPixel.begin();
  keyPixel.setBrightness(kKeyPixelBrightness);
  keyPixel.clear();
  keyPixel.show();
}

bool connected() { return static_cast<bool>(Serial); }

int available() { return Serial.available(); }

int read() { return Serial.read(); }

void write(const char *data, std::size_t size) {
  Serial.write(reinterpret_cast<const std::uint8_t *>(data), size);
}

void flush() { Serial.flush(); }

bool sendKeyboardChord(std::uint8_t modifiers, const KeyboardKeycodes &keys) {
  if (TinyUSBDevice.suspended()) TinyUSBDevice.remoteWakeup();
  return transmitKeyboardReports(
      modifiers, keys, kHidReadyPollLimit,
      []() { return keyboard.ready(); },
      [](const KeyboardReport &keyboardReport) {
        hid_keyboard_report_t report{};
        report.modifier = keyboardReport.modifiers;
        for (std::size_t index = 0; index < keyboardReport.keys.size();
             ++index) {
          report.keycode[index] = keyboardReport.keys[index];
        }
        return keyboard.sendReport(kKeyboardReportId, &report, sizeof(report));
      },
      []() { delay(1); });
}

bool sendHotkey(std::uint8_t modifiers, std::uint8_t keycode) {
  KeyboardKeycodes keys{};
  keys[0] = keycode;
  return sendKeyboardChord(modifiers, keys);
}

bool sendConsumerControl(std::uint16_t usage) {
  if (TinyUSBDevice.suspended()) TinyUSBDevice.remoteWakeup();
  return transmitConsumerReports(
      usage, kHidReadyPollLimit, []() { return keyboard.ready(); },
      [](std::uint16_t reportUsage) {
        return keyboard.sendReport(kConsumerReportId, &reportUsage,
                                   sizeof(reportUsage));
      },
      []() { delay(1); });
}

bool configureDisplay(const std::optional<OledConfig> &config) {
  stopDisplay();
  displayRequested = config.has_value();
  if (!displayRequested) return true;

  switch (selectRp2040OledBus(config->sda, config->scl)) {
    case Rp2040OledBus::I2c0:
      startHardwareI2c(Wire, config->sda, config->scl);
      i2c0Display.reset(
          new (std::nothrow) U8G2_SSD1306_128X32_UNIVISION_F_HW_I2C(
              U8G2_R0, U8X8_PIN_NONE));
      display = i2c0Display.get();
      break;
    case Rp2040OledBus::I2c1:
      startHardwareI2c(Wire1, config->sda, config->scl);
      i2c1Display.reset(
          new (std::nothrow) U8G2_SSD1306_128X32_UNIVISION_F_2ND_HW_I2C(
              U8G2_R0, U8X8_PIN_NONE));
      display = i2c1Display.get();
      break;
    case Rp2040OledBus::Software:
      softwareDisplay.reset(
          new (std::nothrow) U8G2_SSD1306_128X32_UNIVISION_F_SW_I2C(
              U8G2_R0, config->scl, config->sda, U8X8_PIN_NONE));
      display = softwareDisplay.get();
      break;
  }
  if (!display) {
    stopDisplay();
    return false;
  }
  display->setI2CAddress(kOledI2cAddress << 1U);
  display->setBusClock(kOledI2cClockHz);
  // U8g2 exposes no post-begin I2C transfer status to validate later writes.
  if (!display->begin()) {
    stopDisplay();
    return false;
  }
  displayHealthy = true;
  refreshMode =
      selectRefreshMode(kPartialUpdateSupported, kDisplayRotationDegrees);
  display->setFont(u8g2_font_6x13_tf);
  display->clearBuffer();
  display->sendBuffer();
  return true;
}

bool renderLocalDisplay(const DisplayFrame &frame) {
  if (!display || !displayHealthy) return !displayRequested;
  if (displayBufferSource == DisplayBufferSource::Local &&
      lastDisplayFrame.has_value() && *lastDisplayFrame == frame) {
    return true;
  }
  dirtyTiles.clear();
  display->setFont(u8g2_font_6x13_tf);
  display->clearBuffer();
  for (std::size_t index = 0; index < kStatusBaselines.size(); ++index) {
    display->drawStr(0, kStatusBaselines[index], frame.lines[index].c_str());
  }
  if (!frame.lines[2].empty()) {
    const auto inputWidth = display->getStrWidth(frame.lines[2].c_str());
    const auto inputX = inputWidth < kDisplayWidth
                            ? static_cast<std::uint8_t>(kDisplayWidth - inputWidth)
                            : 0;
    display->drawStr(inputX, kInputBaseline, frame.lines[2].c_str());
  }
  display->sendBuffer();
  lastDisplayFrame = frame;
  displayBufferSource = DisplayBufferSource::Local;
  return true;
}

bool renderRemoteDisplay(const RemoteDisplayCommit &scene,
                         bool fullRedraw) {
  if (!display || !displayHealthy) return !displayRequested;
  if (!supportsRemoteScene(scene)) return false;
  lastDisplayFrame.reset();
  const bool redrawAll =
      fullRedraw || displayBufferSource != DisplayBufferSource::Remote;
  if (redrawAll) {
    display->clearBuffer();
  } else {
    display->setDrawColor(0);
    for (std::size_t index = 0; index < scene.dirtyCount; ++index) {
      const auto &bounds = scene.dirtyBounds[index];
      display->drawBox(bounds.x, bounds.y, bounds.width, bounds.height);
    }
    display->setDrawColor(1);
  }

  for (std::size_t index = 0; index < scene.operationCount; ++index) {
    const auto &operation = scene.operations[index];
    if (operation.kind != DisplayOperationKind::Text ||
        operation.fontId != kRemoteDisplayFontId) {
      continue;
    }
    display->setFont(u8g2_font_6x13_tf);
    display->drawStr(operation.x, operation.baselineY,
                     operation.text.c_str());
  }
  if (redrawAll) {
    dirtyTiles.markPixels({0, 0, kDisplayWidth,
                           kDisplayHeightTiles * 8U});
  } else {
    for (std::size_t index = 0; index < scene.dirtyCount; ++index) {
      dirtyTiles.markPixels(scene.dirtyBounds[index]);
    }
  }
  displayBufferSource = DisplayBufferSource::Remote;
  return true;
}

void resetRemoteDisplay() {
  dirtyTiles.clear();
  lastDisplayFrame.reset();
  displayBufferSource = DisplayBufferSource::None;
}

void serviceDisplay() {
  if (!display || !displayHealthy || !dirtyTiles.hasDirty()) return;
  if (refreshMode == RefreshMode::Full) {
    display->sendBuffer();
    dirtyTiles.clear();
    return;
  }
  const auto run = dirtyTiles.takeRun(kDisplayServiceDataBytes);
  if (run.has_value()) {
    display->updateDisplayArea(run->tx, run->ty, run->tw, run->th);
  }
}

void showRandomKeyColor() {
  const auto hue = static_cast<std::uint16_t>(random(0x10000L));
  keyPixel.setPixelColor(0, keyPixel.gamma32(keyPixel.ColorHSV(hue)));
  keyPixel.show();
}

void clearKeyColor() {
  keyPixel.clear();
  keyPixel.show();
}

void delayMs(std::uint32_t milliseconds) { delay(milliseconds); }
}  // namespace platform
