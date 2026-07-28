#include <Arduino.h>
#include <USB.h>
#include <USBCDC.h>
#include <USBHIDKeyboard.h>

#include <string>
#include <vector>

#include "GpioTriggerController.h"
#include "TriggerProtocol.h"

namespace {
constexpr std::size_t kMaxResponseLineLength = 255;
constexpr const char *kHelloLine =
    "HELLO 2 esp32s3 17 0 1 2 3 4 5 6 7 8 9 12 13 14 15 16 17 18\n";

USBCDC usbSerial;
USBHIDKeyboard keyboard;
GpioTriggerController controller;
ResponseLineBuffer responseLines(kMaxResponseLineLength);
TopologyBuilder topologyBuilder;
bool helperConnected = false;

void writeLine(const std::string &line) {
  usbSerial.write(line.c_str(), line.size());
  usbSerial.flush();
}

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

void applyRuntimePinModes() {
  for (const auto gpio : GpioTriggerController::kSupportedPins) {
    pinMode(gpio, INPUT);
  }
  for (const auto &source : controller.topology().directs) {
    for (const auto gpio : source.pins) pinMode(gpio, INPUT_PULLUP);
  }
  for (const auto &source : controller.topology().matrices) {
    for (const auto gpio : source.rows) pinMode(gpio, INPUT_PULLUP);
    for (const auto gpio : source.columns) pinMode(gpio, INPUT_PULLUP);
  }
}

void applyLearningPinModes() {
  for (const auto gpio : GpioTriggerController::kSupportedPins) {
    pinMode(gpio, INPUT);
  }
  for (const auto gpio : controller.learningPins()) {
    pinMode(gpio, INPUT_PULLUP);
  }
}

void configError(std::uint32_t revision, const char *code) {
  writeLine("CONFIG_ERROR " + std::to_string(revision) + " " + code + "\n");
}

void handleResponseLine(std::string_view line, std::uint32_t nowMs) {
  const auto command = parseHelperCommand(line);
  if (!command.has_value()) {
    topologyBuilder.cancel();
    return;
  }

  switch (command->kind) {
    case HelperCommandKind::ConfigBegin:
      if (controller.isLearning() ||
          !topologyBuilder.begin(command->revision, command->debounceMs)) {
        configError(command->revision, "invalid_begin");
      }
      return;
    case HelperCommandKind::ConfigDirect:
      if (!topologyBuilder.addDirect(command->revision, command->sourceIndex,
                                     command->pins)) {
        topologyBuilder.cancel();
        configError(command->revision, "invalid_direct");
      }
      return;
    case HelperCommandKind::ConfigMatrix:
      if (!topologyBuilder.addMatrix(command->revision, command->sourceIndex,
                                     command->rows, command->columns)) {
        topologyBuilder.cancel();
        configError(command->revision, "invalid_matrix");
      }
      return;
    case HelperCommandKind::ConfigCommit: {
      const auto topology = topologyBuilder.commit(command->revision);
      if (!topology.has_value()) {
        configError(command->revision, "invalid_commit");
        return;
      }
      controller.configure(*topology, nowMs);
      applyRuntimePinModes();
      writeLine("CONFIG_OK " + std::to_string(command->revision) + "\n");
      return;
    }
    case HelperCommandKind::LearnBegin:
      topologyBuilder.cancel();
      if (!controller.beginLearning(command->revision, command->pins, nowMs)) {
        configError(command->revision, "invalid_learning");
        return;
      }
      applyLearningPinModes();
      writeLine("LEARN_OK " + std::to_string(command->revision) + "\n");
      return;
    case HelperCommandKind::LearnEnd:
      if (!controller.endLearning(command->revision, nowMs)) {
        configError(command->revision, "invalid_learning_revision");
        return;
      }
      applyRuntimePinModes();
      writeLine("LEARN_OK " + std::to_string(command->revision) + "\n");
      return;
    case HelperCommandKind::Skip:
      controller.acceptStep(command->eventId, 0, 0, false, nowMs);
      return;
    case HelperCommandKind::Paste:
    case HelperCommandKind::Hotkey:
      break;
  }

  if (controller.acceptStep(command->eventId, command->step, command->total,
                            true, nowMs) != ResponseAction::Execute) {
    return;
  }
  if (command->kind == HelperCommandKind::Paste) {
    pasteClipboard();
  } else {
    sendHotkey(command->modifierMask, command->keycode);
  }
  writeLine(formatDone(command->eventId, command->step));
}

void readHelperResponses(std::uint32_t nowMs) {
  while (usbSerial.available() > 0) {
    const int value = usbSerial.read();
    if (value < 0) {
      return;
    }

    const auto line = responseLines.push(static_cast<char>(value));
    if (line.has_value()) handleResponseLine(*line, nowMs);
  }
}

void emitInput(const std::optional<InputEvent> &event, bool learning) {
  if (event.has_value()) {
    writeLine(learning ? formatLearningEvent(*event) : formatInputEvent(*event));
  }
}

void scanRuntimeInputs(std::uint32_t nowMs) {
  for (const auto &source : controller.topology().directs) {
    for (const auto gpio : source.pins) {
      emitInput(controller.updatePin(gpio, digitalRead(gpio) == HIGH, nowMs),
                false);
    }
  }

  for (const auto &source : controller.topology().matrices) {
    for (const auto row : source.rows) {
      pinMode(row, OUTPUT);
      digitalWrite(row, LOW);
      delayMicroseconds(5);
      for (const auto column : source.columns) {
        emitInput(controller.updateContact(source.index, row, column,
                                           digitalRead(column) == LOW, nowMs),
                  false);
      }
      pinMode(row, INPUT_PULLUP);
    }
  }
}

void scanLearningInputs(std::uint32_t nowMs) {
  bool grounded = false;
  for (const auto pin : controller.learningPins()) {
    const bool inputHigh = digitalRead(pin) == HIGH;
    grounded |= !inputHigh;
    emitInput(controller.updateLearningPin(pin, inputHigh, nowMs), true);
  }

  const auto &pins = controller.learningPins();
  for (std::size_t left = 0; left < pins.size(); ++left) {
    for (std::size_t right = left + 1; right < pins.size(); ++right) {
      bool closed = false;
      if (!grounded) {
        pinMode(pins[left], OUTPUT);
        digitalWrite(pins[left], LOW);
        delayMicroseconds(5);
        closed = digitalRead(pins[right]) == LOW;
        pinMode(pins[left], INPUT_PULLUP);
      }
      emitInput(controller.updateLearningContact(pins[left], pins[right],
                                                  closed, nowMs),
                true);
    }
  }
}
}  // namespace

void setup() {
  USB.VID(0x303A);
  USB.PID(0x4002);
  USB.manufacturerName("Kivo");
  USB.productName("Kivo Keyboard");

  usbSerial.begin(115200);
  keyboard.begin();
  USB.begin();
}

void loop() {
  const std::uint32_t nowMs = millis();
  const bool connected = static_cast<bool>(usbSerial);
  if (connected && !helperConnected) writeLine(kHelloLine);
  helperConnected = connected;
  controller.expire(nowMs);
  readHelperResponses(nowMs);
  if (controller.isLearning()) {
    scanLearningInputs(nowMs);
  } else {
    scanRuntimeInputs(nowMs);
  }
  delay(1);
}
