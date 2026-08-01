#include <Adafruit_TinyUSB.h>
#include <Arduino.h>

#include "HidReportTransport.h"
#include "Platform.h"

namespace {
std::uint8_t const kKeyboardDescriptor[] = {TUD_HID_REPORT_DESC_KEYBOARD()};
Adafruit_USBD_HID keyboard(kKeyboardDescriptor, sizeof(kKeyboardDescriptor),
                           HID_ITF_PROTOCOL_KEYBOARD, 2, false);
constexpr std::size_t kHidReadyPollLimit = 100;
}  // namespace

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

void delayMs(std::uint32_t milliseconds) { delay(milliseconds); }
}  // namespace platform
