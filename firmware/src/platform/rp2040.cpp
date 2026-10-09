#include <Adafruit_TinyUSB.h>
#include <Arduino.h>

#include <array>

#include "HidReportTransport.h"
#include "Platform.h"

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
}  // namespace

namespace platform {
const BoardProfile &boardProfile() { return kYdRp2040; }

void begin() {
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

void showRandomKeyColor() {}

void clearKeyColor() {}

void delayMs(std::uint32_t milliseconds) { delay(milliseconds); }
}  // namespace platform
