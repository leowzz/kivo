#include <Adafruit_NeoPixel.h>
#include <Adafruit_TinyUSB.h>
#include <Arduino.h>
#include <U8g2lib.h>
#include <Wire.h>

#include <array>
#include <memory>
#include <optional>

#include "HidReportTransport.h"
#include "Platform.h"
#include "Rp2040OledBus.h"

namespace {
std::uint8_t const kKeyboardDescriptor[] = {TUD_HID_REPORT_DESC_KEYBOARD()};
Adafruit_USBD_HID keyboard(kKeyboardDescriptor, sizeof(kKeyboardDescriptor),
                           HID_ITF_PROTOCOL_KEYBOARD, 2, false);
Adafruit_NeoPixel keyPixel(1, PIN_NEOPIXEL, NEO_GRB + NEO_KHZ800);
constexpr std::size_t kHidReadyPollLimit = 100;
constexpr std::uint8_t kKeyPixelBrightness = 64;
constexpr std::uint8_t kOledI2cAddress = 0x3C;
constexpr std::uint32_t kOledI2cClockHz = 100000;
constexpr std::array<std::uint8_t, 3> kDisplayBaselines = {9, 18, 27};
std::unique_ptr<U8G2_SSD1306_128X32_UNIVISION_F_HW_I2C> i2c0Display;
std::unique_ptr<U8G2_SSD1306_128X32_UNIVISION_F_2ND_HW_I2C> i2c1Display;
std::unique_ptr<U8G2_SSD1306_128X32_UNIVISION_F_SW_I2C> softwareDisplay;
U8G2 *display = nullptr;
TwoWire *displayWire = nullptr;
std::optional<DisplayFrame> lastDisplayFrame;

void stopDisplay() {
  if (display) {
    display->clearBuffer();
    display->sendBuffer();
    display->setPowerSave(1);
  }
  display = nullptr;
  i2c0Display.reset();
  i2c1Display.reset();
  softwareDisplay.reset();
  if (displayWire) displayWire->end();
  displayWire = nullptr;
}

void startHardwareI2c(TwoWire &wire, std::uint8_t sda, std::uint8_t scl) {
  wire.setSDA(sda);
  wire.setSCL(scl);
  wire.begin();
  wire.setClock(kOledI2cClockHz);
  displayWire = &wire;
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

bool sendHotkey(std::uint8_t modifiers, std::uint8_t keycode) {
  if (TinyUSBDevice.suspended()) TinyUSBDevice.remoteWakeup();
  return transmitHotkeyReports(
      modifiers, keycode, kHidReadyPollLimit,
      []() { return keyboard.ready(); },
      [](std::uint8_t reportModifiers, std::uint8_t reportKeycode) {
        hid_keyboard_report_t report{};
        report.modifier = reportModifiers;
        report.keycode[0] = reportKeycode;
        return keyboard.sendReport(0, &report, sizeof(report));
      },
      []() { delay(1); });
}

void configureDisplay(const std::optional<OledConfig> &config) {
  stopDisplay();
  lastDisplayFrame.reset();
  if (!config.has_value()) return;

  switch (selectRp2040OledBus(config->sda, config->scl)) {
    case Rp2040OledBus::I2c0:
      startHardwareI2c(Wire, config->sda, config->scl);
      i2c0Display =
          std::make_unique<U8G2_SSD1306_128X32_UNIVISION_F_HW_I2C>(
              U8G2_R0, U8X8_PIN_NONE);
      display = i2c0Display.get();
      break;
    case Rp2040OledBus::I2c1:
      startHardwareI2c(Wire1, config->sda, config->scl);
      i2c1Display =
          std::make_unique<U8G2_SSD1306_128X32_UNIVISION_F_2ND_HW_I2C>(
              U8G2_R0, U8X8_PIN_NONE);
      display = i2c1Display.get();
      break;
    case Rp2040OledBus::Software:
      softwareDisplay =
          std::make_unique<U8G2_SSD1306_128X32_UNIVISION_F_SW_I2C>(
              U8G2_R0, config->scl, config->sda, U8X8_PIN_NONE);
      display = softwareDisplay.get();
      break;
  }
  display->setI2CAddress(kOledI2cAddress << 1U);
  display->setBusClock(kOledI2cClockHz);
  display->begin();
  display->setFont(u8g2_font_5x7_tf);
  display->clearBuffer();
  display->sendBuffer();
}

void renderDisplay(const DisplayFrame &frame) {
  if (!display || (lastDisplayFrame.has_value() &&
                   *lastDisplayFrame == frame)) {
    return;
  }
  display->clearBuffer();
  for (std::size_t index = 0; index < frame.lines.size(); ++index) {
    display->drawStr(0, kDisplayBaselines[index], frame.lines[index].c_str());
  }
  display->sendBuffer();
  lastDisplayFrame = frame;
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
