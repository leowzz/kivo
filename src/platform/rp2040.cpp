#include <Adafruit_TinyUSB.h>
#include <Arduino.h>

#include "Platform.h"

namespace {
std::uint8_t const kKeyboardDescriptor[] = {TUD_HID_REPORT_DESC_KEYBOARD()};
Adafruit_USBD_HID keyboard(kKeyboardDescriptor, sizeof(kKeyboardDescriptor),
                           HID_ITF_PROTOCOL_KEYBOARD, 2, false);
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
