#include <Arduino.h>
#include <USB.h>
#include <USBCDC.h>
#include <USBHIDKeyboard.h>

#include <string>

#include "GpioTriggerController.h"
#include "TriggerProtocol.h"

namespace {
constexpr std::size_t kMaxResponseLineLength = 31;

USBCDC usbSerial;
USBHIDKeyboard keyboard;
GpioTriggerController controller;
ResponseLineBuffer responseLines(kMaxResponseLineLength);

void pasteClipboard() {
  keyboard.press(KEY_LEFT_GUI);
  keyboard.press('v');
  delay(10);
  keyboard.releaseAll();
}

void sendHotkey(std::uint8_t modifierMask, std::uint8_t keycode) {
  KeyReport report{};
  report.modifiers = modifierMask;
  report.keys[0] = keycode;
  keyboard.sendReport(&report);
  delay(10);
  keyboard.releaseAll();
}

void handleResponseLine(std::string_view line) {
  const auto response = parseHelperResponse(line);
  if (!response.has_value()) {
    return;
  }

  const bool execute = response->kind != HelperResponseKind::Skip;
  if (controller.handleResponse(response->eventId, execute) !=
      ResponseAction::Execute) {
    return;
  }
  if (response->kind == HelperResponseKind::Paste) {
    pasteClipboard();
  } else if (response->kind == HelperResponseKind::Hotkey) {
    sendHotkey(response->modifierMask, response->keycode);
  }
}

void readHelperResponses() {
  while (usbSerial.available() > 0) {
    const int value = usbSerial.read();
    if (value < 0) {
      return;
    }

    const auto line = responseLines.push(static_cast<char>(value));
    if (line.has_value()) handleResponseLine(*line);
  }
}

void scanInputs(std::uint32_t nowMs) {
  for (const std::uint8_t gpio : GpioTriggerController::kSupportedPins) {
    const bool inputHigh = digitalRead(gpio) == HIGH;
    const auto event = controller.updatePin(gpio, inputHigh, nowMs);
    if (!event.has_value()) {
      continue;
    }

    const std::string message = formatPressEvent(*event);
    usbSerial.write(message.c_str(), message.size());
    usbSerial.flush();
  }
}
}  // namespace

void setup() {
  for (const std::uint8_t gpio : GpioTriggerController::kSupportedPins) {
    pinMode(gpio, INPUT_PULLUP);
  }

  USB.VID(0x303A);
  USB.PID(0x4002);
  USB.manufacturerName("ESP Vibe");
  USB.productName("ESP Vibe Text Keyboard");

  usbSerial.begin(115200);
  keyboard.begin();
  USB.begin();
}

void loop() {
  const std::uint32_t nowMs = millis();
  controller.expire(nowMs);
  readHelperResponses();
  scanInputs(nowMs);
  delay(1);
}
