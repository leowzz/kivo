#include <Arduino.h>
#include <USB.h>
#include <USBCDC.h>
#include <USBHID.h>
#include <USBHIDKeyboard.h>
#include <USBHIDConsumerControl.h>

#include "platform/HidReportTransport.h"
#include "platform/Platform.h"

namespace {
constexpr std::size_t kHidReadyPollLimit = 100;
USBCDC usbSerial;
USBHIDKeyboard keyboard;
USBHID keyboardTransport;
USBHIDConsumerControl consumerControl;

bool sendKeyboardReport(const platform::KeyboardReport &keyboardReport) {
  KeyReport report{};
  report.modifiers = keyboardReport.modifiers;
  for (std::size_t index = 0; index < keyboardReport.keys.size(); ++index) {
    report.keys[index] = keyboardReport.keys[index];
  }
  hid_keyboard_report_t hidReport{};
  hidReport.modifier = report.modifiers;
  for (std::size_t index = 0; index < keyboardReport.keys.size(); ++index) {
    hidReport.keycode[index] = report.keys[index];
  }
  return keyboardTransport.SendReport(HID_REPORT_ID_KEYBOARD, &hidReport,
                                      sizeof(hidReport));
}
}  // namespace

namespace platform {
const BoardProfile &boardProfile() { return kYdEsp32S3; }

void begin() {
  USB.VID(0x303A);
  USB.PID(0x4002);
  USB.manufacturerName("Kivo");
  USB.productName("Kivo Keyboard");

  usbSerial.begin(115200);
  keyboard.begin();
  consumerControl.begin();
  USB.begin();
}

bool connected() { return static_cast<bool>(usbSerial); }

int available() { return usbSerial.available(); }

int read() { return usbSerial.read(); }

void write(const char *data, std::size_t size) { usbSerial.write(data, size); }

void flush() { usbSerial.flush(); }

bool sendHotkey(std::uint8_t modifiers, std::uint8_t keycode) {
  KeyboardKeycodes keys{};
  keys[0] = keycode;
  return sendKeyboardChord(modifiers, keys);
}

bool sendKeyboardChord(std::uint8_t modifiers, const KeyboardKeycodes &keys) {
  return transmitKeyboardReports(
      modifiers, keys, kHidReadyPollLimit,
      []() { return keyboardTransport.ready(); }, sendKeyboardReport,
      []() { delay(1); });
}

bool sendConsumerControl(std::uint16_t usage) {
  const bool pressed = consumerControl.press(usage) > 0;
  delay(10);
  const bool released = consumerControl.release() > 0;
  return pressed && released;
}

bool configureDisplay(const std::optional<OledConfig> &config) {
  return !config.has_value();
}

void setDisplayBrightness(std::uint8_t) {}

bool renderLocalDisplay(const DisplayFrame &) { return true; }

bool renderRemoteDisplay(const RemoteDisplayCommit &, bool) { return true; }

void resetRemoteDisplay() {}

void serviceDisplay() {}

void showRandomKeyColor() {}

void clearKeyColor() {}

void delayMs(std::uint32_t milliseconds) { delay(milliseconds); }
}  // namespace platform
