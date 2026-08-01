#include <Arduino.h>
#include <USB.h>
#include <USBCDC.h>
#include <USBHIDKeyboard.h>

#include "platform/Platform.h"

namespace {
USBCDC usbSerial;
USBHIDKeyboard keyboard;
}  // namespace

namespace platform {
const BoardProfile &boardProfile() { return kLuatOsEsp32S3Aio; }

void begin() {
  USB.VID(0x303A);
  USB.PID(0x4002);
  USB.manufacturerName("Kivo");
  USB.productName("Kivo Keyboard");

  usbSerial.begin(115200);
  keyboard.begin();
  USB.begin();
}

bool connected() { return static_cast<bool>(usbSerial); }

int available() { return usbSerial.available(); }

int read() { return usbSerial.read(); }

void write(const char *data, std::size_t size) { usbSerial.write(data, size); }

void flush() { usbSerial.flush(); }

bool sendHotkey(std::uint8_t modifiers, std::uint8_t keycode) {
  KeyReport report{};
  report.modifiers = modifiers;
  report.keys[0] = keycode;
  keyboard.sendReport(&report);
  delay(10);
  keyboard.releaseAll();
  return true;
}

void delayMs(std::uint32_t milliseconds) { delay(milliseconds); }
}  // namespace platform
