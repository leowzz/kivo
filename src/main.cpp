#include <Arduino.h>

#include <string>
#include <vector>

#include "DisplayStatus.h"
#include "GpioTriggerController.h"
#include "Handshake.h"
#include "KeyActivityIndicator.h"
#include "TriggerProtocol.h"
#include "platform/Platform.h"

namespace {
constexpr std::size_t kMaxResponseLineLength = 255;
std::string helloLine;

GpioTriggerController controller(platform::boardProfile());
KeyActivityIndicator keyIndicator;
ResponseLineBuffer responseLines(kMaxResponseLineLength);
TopologyBuilder topologyBuilder(platform::boardProfile());
DisplayStatusModel displayStatus;
bool helperConnected = false;

void writeLine(const std::string &line) {
  platform::write(line.c_str(), line.size());
  platform::flush();
}

bool pasteClipboard() {
  return platform::sendHotkey(0x08, 0x19);
}

void renderStatus() { platform::renderDisplay(displayStatus.frame()); }

bool isActiveOledPin(std::uint8_t pin) {
  const auto &oled = controller.topology().oled;
  return oled.has_value() && (pin == oled->sda || pin == oled->scl);
}

void applyRuntimePinModes() {
  const auto &profile = platform::boardProfile();
  for (std::size_t index = 0; index < profile.safePinCount; ++index) {
    const auto pin = profile.safePins[index];
    if (!isActiveOledPin(pin)) pinMode(pin, INPUT);
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
  const auto &profile = platform::boardProfile();
  for (std::size_t index = 0; index < profile.safePinCount; ++index) {
    const auto pin = profile.safePins[index];
    if (!isActiveOledPin(pin)) pinMode(pin, INPUT);
  }
  for (const auto gpio : controller.learningPins()) {
    pinMode(gpio, INPUT_PULLUP);
  }
}

void configError(std::uint32_t revision, const char *code) {
  displayStatus.setConfigError();
  renderStatus();
  writeLine("CONFIG_ERROR " + std::to_string(revision) + " " + code + "\n");
}

void resetKeyIndicator() {
  keyIndicator.reset();
  platform::clearKeyColor();
}

void handleResponseLine(std::string_view line, std::uint32_t nowMs) {
  const auto command = parseHelperCommand(line);
  if (!command.has_value()) {
    topologyBuilder.cancel();
    return;
  }

  switch (command->kind) {
    case HelperCommandKind::Hello:
      writeLine(helloLine);
      return;
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
    case HelperCommandKind::ConfigOled:
      if (!topologyBuilder.addOled(command->revision, command->oledSda,
                                   command->oledScl)) {
        topologyBuilder.cancel();
        configError(command->revision, "invalid_oled");
      }
      return;
    case HelperCommandKind::ConfigCommit: {
      const auto topology = topologyBuilder.commit(command->revision);
      if (!topology.has_value()) {
        configError(command->revision, "invalid_commit");
        return;
      }
      resetKeyIndicator();
      controller.configure(*topology, nowMs);
      platform::configureDisplay(topology->oled);
      applyRuntimePinModes();
      displayStatus.setReady(topology->keyCount());
      displayStatus.clearLastInput();
      renderStatus();
      writeLine("CONFIG_OK " + std::to_string(command->revision) + "\n");
      return;
    }
    case HelperCommandKind::LearnBegin:
      topologyBuilder.cancel();
      if (!controller.beginLearning(command->revision, command->pins, nowMs)) {
        configError(command->revision, "invalid_learning");
        return;
      }
      resetKeyIndicator();
      applyLearningPinModes();
      displayStatus.setLearning(command->pins.size());
      renderStatus();
      writeLine("LEARN_OK " + std::to_string(command->revision) + "\n");
      return;
    case HelperCommandKind::LearnEnd:
      if (!controller.endLearning(command->revision, nowMs)) {
        configError(command->revision, "invalid_learning_revision");
        return;
      }
      resetKeyIndicator();
      applyRuntimePinModes();
      displayStatus.setReady(controller.topology().keyCount());
      displayStatus.clearLastInput();
      renderStatus();
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
  const bool sent = command->kind == HelperCommandKind::Paste
                        ? pasteClipboard()
                        : platform::sendHotkey(command->modifierMask,
                                               command->keycode);
  if (!sent) return;
  writeLine(formatDone(command->eventId, command->step));
}

void readHelperResponses(std::uint32_t nowMs) {
  while (platform::available() > 0) {
    const int value = platform::read();
    if (value < 0) {
      return;
    }

    const auto line = responseLines.push(static_cast<char>(value));
    if (line.has_value()) handleResponseLine(*line, nowMs);
  }
}

void emitInput(const std::optional<InputEvent> &event, bool learning) {
  if (event.has_value()) {
    displayStatus.recordInput(*event);
    renderStatus();
    switch (keyIndicator.handle(event->state)) {
      case KeyIndicatorAction::ShowRandomColor:
        platform::showRandomKeyColor();
        break;
      case KeyIndicatorAction::Off:
        platform::clearKeyColor();
        break;
      case KeyIndicatorAction::None:
        break;
    }
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
  helloLine = formatHello(platform::boardProfile(), KIVO_FIRMWARE_BUILD_ID);
  platform::begin();
}

void loop() {
  const std::uint32_t nowMs = millis();
  const bool connected = platform::connected();
  if (connected != helperConnected) {
    displayStatus.setUsbConnected(connected);
    renderStatus();
    if (connected) writeLine(helloLine);
  }
  helperConnected = connected;
  controller.expire(nowMs);
  readHelperResponses(nowMs);
  if (controller.isLearning()) {
    scanLearningInputs(nowMs);
  } else {
    scanRuntimeInputs(nowMs);
  }
  platform::delayMs(1);
}
