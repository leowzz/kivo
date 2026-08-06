#include <Adafruit_NeoPixel.h>
#include <Adafruit_TinyUSB.h>
#include <Arduino.h>
#include <U8x8lib.h>

#include <array>
#include <memory>
#include <optional>

#include "HidReportTransport.h"
#include "Platform.h"

namespace {
std::uint8_t const kKeyboardDescriptor[] = {TUD_HID_REPORT_DESC_KEYBOARD()};
Adafruit_USBD_HID keyboard(kKeyboardDescriptor, sizeof(kKeyboardDescriptor),
                           HID_ITF_PROTOCOL_KEYBOARD, 2, false);
Adafruit_NeoPixel keyPixel(1, PIN_NEOPIXEL, NEO_GRB + NEO_KHZ800);
constexpr std::size_t kHidReadyPollLimit = 100;
constexpr std::uint8_t kKeyPixelBrightness = 64;
constexpr std::uint8_t kOledI2cAddress = 0x3C;
constexpr std::array<std::uint8_t, 3> kDisplayRows = {0, 1, 3};
std::unique_ptr<U8X8_SSD1306_128X32_UNIVISION_SW_I2C> display;
std::optional<DisplayFrame> lastDisplayFrame;
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
  if (display) {
    display->clearDisplay();
    display->setPowerSave(1);
  }
  display.reset();
  lastDisplayFrame.reset();
  if (!config.has_value()) return;

  display = std::make_unique<U8X8_SSD1306_128X32_UNIVISION_SW_I2C>(
      config->scl, config->sda, U8X8_PIN_NONE);
  display->setI2CAddress(kOledI2cAddress << 1U);
  display->begin();
  display->setFont(u8x8_font_chroma48medium8_r);
  display->clearDisplay();
}

void renderDisplay(const DisplayFrame &frame) {
  if (!display || (lastDisplayFrame.has_value() &&
                   *lastDisplayFrame == frame)) {
    return;
  }
  for (std::size_t index = 0; index < frame.lines.size(); ++index) {
    if (!lastDisplayFrame.has_value() ||
        lastDisplayFrame->lines[index] != frame.lines[index]) {
      display->drawString(0, kDisplayRows[index], frame.lines[index].c_str());
    }
  }
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
