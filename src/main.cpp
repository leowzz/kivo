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
std::string responseLine;

void pasteClipboard() {
  keyboard.press(KEY_LEFT_GUI);
  keyboard.press('v');
  delay(10);
  keyboard.releaseAll();
}

void handleResponseLine() {
  const auto response = parseHelperResponse(responseLine);
  responseLine.clear();
  if (!response.has_value()) {
    return;
  }

  const bool paste = response->kind == HelperResponseKind::Paste;
  if (controller.handleResponse(response->eventId, paste) ==
      ResponseAction::Paste) {
    pasteClipboard();
  }
}

void readHelperResponses() {
  while (usbSerial.available() > 0) {
    const int value = usbSerial.read();
    if (value < 0) {
      return;
    }

    const char character = static_cast<char>(value);
    if (character == '\n') {
      responseLine.push_back(character);
      handleResponseLine();
    } else if (responseLine.size() < kMaxResponseLineLength) {
      responseLine.push_back(character);
    } else {
      responseLine.clear();
    }
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
