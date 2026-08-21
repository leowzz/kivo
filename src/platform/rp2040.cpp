#include <Adafruit_TinyUSB.h>
#include <Arduino.h>
#include <EEPROM.h>
#include <U8g2lib.h>
#include <Wire.h>

#include <algorithm>
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
constexpr std::size_t kHidReadyPollLimit = 100;
constexpr std::uint8_t kOledI2cAddress = 0x3C;
constexpr std::uint32_t kOledI2cClockHz = 400000;
constexpr std::uint8_t kDisplayWidth = 128;
constexpr std::uint8_t kDisplayWidthTiles = 16;
constexpr std::uint8_t kSsd1306HeightTiles = 4;
constexpr std::uint8_t kSh1106HeightTiles = 8;
constexpr std::size_t kDisplayServiceDataBytes = 64;
constexpr bool kPartialUpdateSupported = true;
constexpr std::uint16_t kDisplayRotationDegrees = 0;
constexpr std::array<std::uint8_t, 2> kSsd1306StatusBaselines = {10, 29};
constexpr std::uint8_t kSsd1306InputBaseline = 20;
constexpr std::array<std::uint8_t, 4> kSh1106LocalBaselines = {12, 28, 44, 60};
constexpr std::uint8_t kDisplayBrightnessMagic = 0x4B;
constexpr std::uint8_t kMinimumDisplayBrightnessPercent = 5;
constexpr std::uint8_t kMaximumDisplayBrightnessPercent = 100;
struct PersistedDisplaySettings {
  std::uint8_t magic;
  std::uint8_t brightnessPercent;
  std::uint8_t checksum;
  std::uint8_t reserved;
};
static_assert(sizeof(PersistedDisplaySettings) == 4);
constexpr std::size_t kDisplaySettingsEepromSize =
    sizeof(PersistedDisplaySettings);
std::unique_ptr<U8G2_SSD1306_128X32_UNIVISION_F_HW_I2C> ssd1306I2c0Display;
std::unique_ptr<U8G2_SSD1306_128X32_UNIVISION_F_2ND_HW_I2C> ssd1306I2c1Display;
std::unique_ptr<U8G2_SSD1306_128X32_UNIVISION_F_SW_I2C> ssd1306SoftwareDisplay;
std::unique_ptr<U8G2_SH1106_128X64_NONAME_F_HW_I2C> sh1106I2c0Display;
std::unique_ptr<U8G2_SH1106_128X64_NONAME_F_2ND_HW_I2C> sh1106I2c1Display;
std::unique_ptr<U8G2_SH1106_128X64_NONAME_F_SW_I2C> sh1106SoftwareDisplay;
U8G2 *display = nullptr;
TwoWire *displayWire = nullptr;
OledDriver displayDriver = OledDriver::Ssd1306;
std::uint8_t displayHeightTiles = kSsd1306HeightTiles;
std::optional<DisplayFrame> lastDisplayFrame;
enum class DisplayBufferSource { None, Local, Remote };
DisplayBufferSource displayBufferSource = DisplayBufferSource::None;
bool displayRequested = false;
bool displayHealthy = false;
DirtyTiles dirtyTiles(kDisplayWidthTiles, kSh1106HeightTiles);
RefreshMode refreshMode = RefreshMode::Full;

std::uint8_t displaySettingsChecksum(
    const PersistedDisplaySettings &settings) {
  return static_cast<std::uint8_t>(settings.magic ^ settings.brightnessPercent ^
                                   0xA5U);
}

bool validDisplaySettings(const PersistedDisplaySettings &settings) {
  return settings.magic == kDisplayBrightnessMagic &&
         settings.brightnessPercent >= kMinimumDisplayBrightnessPercent &&
         settings.brightnessPercent <= kMaximumDisplayBrightnessPercent &&
         settings.checksum == displaySettingsChecksum(settings);
}

void stopDisplay() {
  dirtyTiles.clear();
  if (display && displayHealthy) {
    display->clearBuffer();
    display->sendBuffer();
    display->setPowerSave(1);
  }
  displayHealthy = false;
  display = nullptr;
  ssd1306I2c0Display.reset();
  ssd1306I2c1Display.reset();
  ssd1306SoftwareDisplay.reset();
  sh1106I2c0Display.reset();
  sh1106I2c1Display.reset();
  sh1106SoftwareDisplay.reset();
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

const std::uint8_t *remoteDisplayFont(std::uint8_t fontId) {
  switch (fontId) {
    case 0:
      return u8g2_font_6x13_tf;
    case 1:
      return u8g2_font_9x18_tf;
    case 2:
      return u8g2_font_10x20_tf;
    default:
      return nullptr;
  }
}

bool supportsRemoteScene(const RemoteDisplayCommit &scene) {
  if (scene.regionCount > kMaxDisplayRegions ||
      scene.operationCount > kMaxDisplayOps ||
      scene.dirtyCount > kMaxDisplayRegions) {
    return false;
  }
  const auto displayHeight = static_cast<std::uint16_t>(displayHeightTiles * 8U);
  for (std::size_t index = 0; index < scene.regionCount; ++index) {
    const auto &bounds = scene.regions[index].bounds;
    if (bounds.x + bounds.width > kDisplayWidth ||
        bounds.y + bounds.height > displayHeight) {
      return false;
    }
  }
  for (std::size_t index = 0; index < scene.operationCount; ++index) {
    const auto &operation = scene.operations[index];
    if (operation.kind == DisplayOperationKind::Clear) continue;
    if (operation.kind != DisplayOperationKind::Text ||
        operation.fontId > kRemoteDisplayMaxFontId ||
        remoteDisplayFont(operation.fontId) == nullptr ||
        operation.x >= kDisplayWidth || operation.baselineY > displayHeight) {
      return false;
    }
  }
  return true;
}

}  // namespace

namespace platform {
const BoardProfile &boardProfile() { return kYdRp2040; }

void begin() {
  EEPROM.begin(kDisplaySettingsEepromSize);
  TinyUSBDevice.setID(0x2e8a, 0x102e);
  TinyUSBDevice.setManufacturerDescriptor("YD");
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

std::uint8_t loadDisplayBrightness() {
  PersistedDisplaySettings settings{};
  EEPROM.get(0, settings);
  return validDisplaySettings(settings) ? settings.brightnessPercent
                                        : kMaximumDisplayBrightnessPercent;
}

void saveDisplayBrightness(std::uint8_t percent) {
  const auto clamped = std::min<std::uint16_t>(
      std::max<std::uint16_t>(percent, kMinimumDisplayBrightnessPercent),
      kMaximumDisplayBrightnessPercent);
  PersistedDisplaySettings settings{
      kDisplayBrightnessMagic,
      static_cast<std::uint8_t>(clamped),
      0,
      0,
  };
  settings.checksum = displaySettingsChecksum(settings);
  EEPROM.put(0, settings);
  EEPROM.commit();
}

bool configureDisplay(const std::optional<OledConfig> &config) {
  stopDisplay();
  displayRequested = config.has_value();
  if (!displayRequested) return true;
  displayDriver = config->driver;
  displayHeightTiles = displayDriver == OledDriver::Sh1106
                           ? kSh1106HeightTiles
                           : kSsd1306HeightTiles;

  switch (selectRp2040OledBus(config->sda, config->scl)) {
    case Rp2040OledBus::I2c0:
      startHardwareI2c(Wire, config->sda, config->scl);
      if (displayDriver == OledDriver::Sh1106) {
        sh1106I2c0Display.reset(
            new (std::nothrow) U8G2_SH1106_128X64_NONAME_F_HW_I2C(
                U8G2_R0, U8X8_PIN_NONE));
        display = sh1106I2c0Display.get();
      } else {
        ssd1306I2c0Display.reset(
            new (std::nothrow) U8G2_SSD1306_128X32_UNIVISION_F_HW_I2C(
                U8G2_R0, U8X8_PIN_NONE));
        display = ssd1306I2c0Display.get();
      }
      break;
    case Rp2040OledBus::I2c1:
      startHardwareI2c(Wire1, config->sda, config->scl);
      if (displayDriver == OledDriver::Sh1106) {
        sh1106I2c1Display.reset(
            new (std::nothrow) U8G2_SH1106_128X64_NONAME_F_2ND_HW_I2C(
                U8G2_R0, U8X8_PIN_NONE));
        display = sh1106I2c1Display.get();
      } else {
        ssd1306I2c1Display.reset(
            new (std::nothrow) U8G2_SSD1306_128X32_UNIVISION_F_2ND_HW_I2C(
                U8G2_R0, U8X8_PIN_NONE));
        display = ssd1306I2c1Display.get();
      }
      break;
    case Rp2040OledBus::Software:
      if (displayDriver == OledDriver::Sh1106) {
        sh1106SoftwareDisplay.reset(
            new (std::nothrow) U8G2_SH1106_128X64_NONAME_F_SW_I2C(
                U8G2_R0, config->scl, config->sda, U8X8_PIN_NONE));
        display = sh1106SoftwareDisplay.get();
      } else {
        ssd1306SoftwareDisplay.reset(
            new (std::nothrow) U8G2_SSD1306_128X32_UNIVISION_F_SW_I2C(
                U8G2_R0, config->scl, config->sda, U8X8_PIN_NONE));
        display = ssd1306SoftwareDisplay.get();
      }
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

void setDisplayBrightness(std::uint8_t percent) {
  if (!display || !displayHealthy) return;
  const auto clamped = std::min<std::uint16_t>(percent, 100);
  const auto contrast = static_cast<std::uint8_t>(
      (clamped * 255U + 50U) / 100U);
  display->setContrast(contrast);
}

bool renderLocalDisplay(const DisplayFrame &frame) {
  if (!display || !displayHealthy) return !displayRequested;
  if (displayBufferSource == DisplayBufferSource::Local &&
      lastDisplayFrame.has_value() && *lastDisplayFrame == frame) {
    return true;
  }
  display->setFont(u8g2_font_6x13_tf);
  display->clearBuffer();
  if (displayDriver == OledDriver::Sh1106) {
    for (std::size_t index = 0; index < kSh1106LocalBaselines.size(); ++index) {
      display->drawStr(0, kSh1106LocalBaselines[index],
                       frame.lines[index].c_str());
    }
  } else {
    for (std::size_t index = 0; index < kSsd1306StatusBaselines.size();
         ++index) {
      display->drawStr(0, kSsd1306StatusBaselines[index],
                       frame.lines[index].c_str());
    }
    if (!frame.lines[2].empty()) {
      const auto inputWidth = display->getStrWidth(frame.lines[2].c_str());
      const auto inputX = inputWidth < kDisplayWidth
                              ? static_cast<std::uint8_t>(kDisplayWidth - inputWidth)
                              : 0;
      display->drawStr(inputX, kSsd1306InputBaseline,
                       frame.lines[2].c_str());
    }
  }
  // Local frames are infrequent and must be visible immediately for the
  // offline control panel. Keep tile-level updates for remote scenes below.
  display->sendBuffer();
  dirtyTiles.clear();
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
        operation.fontId > kRemoteDisplayMaxFontId) {
      continue;
    }
    const auto *font = remoteDisplayFont(operation.fontId);
    if (!font) return false;
    display->setFont(font);
    display->drawStr(operation.x, operation.baselineY,
                     operation.text.c_str());
  }
  if (redrawAll) {
    dirtyTiles.markPixels(
        {0, 0, kDisplayWidth,
         static_cast<std::uint16_t>(displayHeightTiles * 8U)});
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

void showRandomKeyColor() {}

void clearKeyColor() {}

void delayMs(std::uint32_t milliseconds) { delay(milliseconds); }
}  // namespace platform
