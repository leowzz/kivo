#include <Arduino.h>

#include <string>
#include <optional>
#include <vector>

#include "ActionRunController.h"
#include "ActionRunDispatcher.h"
#include "DisplayController.h"
#include "DisplayStatus.h"
#include "GpioTriggerController.h"
#include "Handshake.h"
#include "KeyActivityIndicator.h"
#include "RemoteDisplay.h"
#include "StandaloneDebugTopology.h"
#include "TriggerProtocol.h"
#include "platform/Platform.h"

namespace {
constexpr std::size_t kMaxResponseLineLength = 255;
constexpr std::uint32_t kStandaloneDisplayStartupDelayMs = 500;
std::string helloLine;

GpioTriggerController controller(platform::boardProfile());
ActionRunController actionRuns;
KeyActivityIndicator keyIndicator;
ResponseLineBuffer responseLines(kMaxResponseLineLength);
TopologyBuilder topologyBuilder(platform::boardProfile());
DisplayStatusModel displayStatus;
DisplayController displayController;
std::optional<RemoteDisplay> remoteDisplay{std::in_place};
bool helperConnected = false;
bool standaloneDisplayPending = false;
std::uint32_t standaloneDisplayStartedMs = 0;

struct PendingDelay {
  std::uint32_t runId;
  std::uint16_t step;
  std::uint16_t total;
  std::uint32_t startedMs;
  std::uint32_t durationMs;
};

std::optional<PendingDelay> pendingDelay;

void writeLine(const std::string &line) {
  platform::write(line.c_str(), line.size());
  platform::flush();
}

bool pasteClipboard() {
  return platform::sendHotkey(0x08, 0x19);
}

void applyDisplayUpdate(const DisplayUpdate &update) {
  if (update.kind == DisplayUpdateKind::Local && update.local != nullptr) {
    platform::renderLocalDisplay(*update.local);
  } else if (update.kind == DisplayUpdateKind::Remote &&
             update.remote != nullptr) {
    platform::renderRemoteDisplay(*update.remote, update.fullRedraw);
  }
}

void showStatus(LocalDisplayPriority priority) {
  applyDisplayUpdate(displayController.showLocal(displayStatus.frame(),
                                                 priority));
}

DisplayFrame helperOfflineFrame() {
  auto frame = displayStatus.frame();
  frame.lines[1] = "HELPER OFFLINE  ";
  frame.lines[2].clear();
  return frame;
}

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
  showStatus(LocalDisplayPriority::Critical);
  writeLine("CONFIG_ERROR " + std::to_string(revision) + " " + code + "\n");
}

void resetKeyIndicator() {
  keyIndicator.reset();
  platform::clearKeyColor();
}

void applyTopologyState(const RuntimeTopology &topology, std::uint32_t nowMs) {
  resetKeyIndicator();
  pendingDelay.reset();
  actionRuns.reset();
  controller.configure(topology, nowMs);
  applyRuntimePinModes();
  displayStatus.setReady(topology.keyCount());
  displayStatus.clearLastInput();
}

void activateTopology(const RuntimeTopology &topology, std::uint32_t nowMs) {
  standaloneDisplayPending = false;
  displayStatus.setStandaloneDebug(false);
  // Release the old display before its I2C pins are reassigned by the topology.
  platform::configureDisplay(topology.oled);
  applyTopologyState(topology, nowMs);
  applyDisplayUpdate(displayController.showLocal(
      displayStatus.frame(), LocalDisplayPriority::Normal));
  applyDisplayUpdate(displayController.clearLocalOverride());
}

void activateStandaloneTopology(const RuntimeTopology &topology,
                                std::uint32_t nowMs) {
  displayStatus.setStandaloneDebug(true);
  applyTopologyState(topology, nowMs);
  standaloneDisplayPending = true;
  standaloneDisplayStartedMs = nowMs;
}

void initializeStandaloneDisplay(std::uint32_t nowMs) {
  if (!standaloneDisplayPending ||
      nowMs - standaloneDisplayStartedMs < kStandaloneDisplayStartupDelayMs) {
    return;
  }
  standaloneDisplayPending = false;
  // Let TinyUSB service its first cycles and the OLED power stabilize first.
  platform::configureDisplay(controller.topology().oled);
  showStatus(LocalDisplayPriority::Startup);
}

void handleResponseLine(std::string_view line, std::uint32_t nowMs) {
  const auto command = parseHelperCommand(line);
  if (!command.has_value()) {
    topologyBuilder.cancel();
    const auto displayError =
        discardMalformedDisplayCommand(*remoteDisplay, line);
    if (displayError.has_value()) writeLine(*displayError);
    return;
  }
  switch (command->kind) {
    case HelperCommandKind::Hello:
      writeLine(helloLine);
      return;
    case HelperCommandKind::ConfigBegin:
      pendingDelay.reset();
      actionRuns.reset();
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
      activateTopology(*topology, nowMs);
      writeLine("CONFIG_OK " + std::to_string(command->revision) + "\n");
      return;
    }
    case HelperCommandKind::DisplayBegin:
    case HelperCommandKind::DisplayRegion:
    case HelperCommandKind::DisplayClear:
    case HelperCommandKind::DisplayText:
    case HelperCommandKind::DisplayCommit: {
      const auto reply = dispatchDisplayCommand(
          *remoteDisplay, *command, controller.topology().oled.has_value());
      if (command->kind == HelperCommandKind::DisplayCommit &&
          reply == formatDisplayOk(command->revision) &&
          remoteDisplay->lastCommit().has_value()) {
        applyDisplayUpdate(
            displayController.commitRemote(*remoteDisplay->lastCommit()));
      }
      if (reply.has_value()) writeLine(*reply);
      return;
    }
    case HelperCommandKind::LearnBegin:
      topologyBuilder.cancel();
      pendingDelay.reset();
      actionRuns.reset();
      if (!controller.beginLearning(command->revision, command->pins, nowMs)) {
        configError(command->revision, "invalid_learning");
        return;
      }
      resetKeyIndicator();
      applyLearningPinModes();
      displayStatus.setLearning(command->pins.size());
      showStatus(LocalDisplayPriority::Critical);
      writeLine("LEARN_OK " + std::to_string(command->revision) + "\n");
      return;
    case HelperCommandKind::LearnEnd:
      if (!controller.endLearning(command->revision, nowMs)) {
        configError(command->revision, "invalid_learning_revision");
        return;
      }
      pendingDelay.reset();
      actionRuns.reset();
      resetKeyIndicator();
      applyRuntimePinModes();
      displayStatus.setReady(controller.topology().keyCount());
      displayStatus.clearLastInput();
      displayController.showLocal(displayStatus.frame(),
                                  LocalDisplayPriority::Normal);
      applyDisplayUpdate(displayController.clearLocalOverride());
      writeLine("LEARN_OK " + std::to_string(command->revision) + "\n");
      return;
    case HelperCommandKind::Skip:
      if (pendingDelay.has_value() &&
          pendingDelay->runId == command->runId) {
        pendingDelay.reset();
      }
      actionRuns.cancel(command->runId);
      return;
    case HelperCommandKind::Paste:
    case HelperCommandKind::Hotkey:
    case HelperCommandKind::Media:
    case HelperCommandKind::Host:
      break;
    case HelperCommandKind::Chord:
      executeKeyboardChord(
          actionRuns, *command, nowMs,
          [](std::uint8_t modifiers, const platform::KeyboardKeycodes &keys) {
            return platform::sendKeyboardChord(modifiers, keys);
          },
          [](std::uint32_t runId, std::uint16_t step) {
            writeLine(formatDone(runId, step));
          });
      return;
    case HelperCommandKind::Delay:
      if (pendingDelay.has_value() ||
          actionRuns.acceptStep(command->runId, command->step, command->total,
                                nowMs) != ResponseAction::Execute) {
        return;
      }
      pendingDelay = PendingDelay{command->runId, command->step, command->total,
                                  nowMs, command->durationMs};
      return;
  }

  if (actionRuns.acceptStep(command->runId, command->step, command->total,
                            nowMs) != ResponseAction::Execute) {
    return;
  }
  bool sent = true;
  if (command->kind == HelperCommandKind::Paste) {
    sent = pasteClipboard();
  } else if (command->kind == HelperCommandKind::Hotkey) {
    sent = platform::sendHotkey(command->modifierMask, command->keycode);
  } else if (command->kind == HelperCommandKind::Media) {
    sent = platform::sendConsumerControl(command->consumerUsage);
  }
  if (!sent) return;
  writeLine(formatDone(command->runId, command->step));
}

void servicePendingDelay(std::uint32_t nowMs) {
  if (!pendingDelay.has_value()) return;
  const auto delay = *pendingDelay;
  if (delay.step < delay.total && !actionRuns.keepAlive(delay.runId, nowMs)) {
    pendingDelay.reset();
    return;
  }
  if (nowMs - delay.startedMs < delay.durationMs) {
    return;
  }
  pendingDelay.reset();
  writeLine(formatDone(delay.runId, delay.step));
}

void readHelperResponses(std::uint32_t nowMs) {
  while (platform::available() > 0) {
    const int value = platform::read();
    if (value < 0) {
      return;
    }

    const auto line = responseLines.push(static_cast<char>(value));
    if (!line.has_value()) continue;
    if (line->overflow) {
      const auto displayError =
          discardMalformedDisplayCommand(*remoteDisplay, line->line);
      if (displayError.has_value()) writeLine(*displayError);
      continue;
    }
    handleResponseLine(line->line, nowMs);
  }
}

void emitInput(const std::optional<InputEvent> &event, bool learning) {
  if (event.has_value()) {
    displayStatus.recordInput(*event);
    showStatus(learning ? LocalDisplayPriority::Critical
                        : LocalDisplayPriority::Normal);
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
  const auto debugTopology =
      makeRp2040StandaloneDebugTopology(platform::boardProfile());
  if (debugTopology.has_value()) {
    activateStandaloneTopology(*debugTopology, millis());
  }
}

void loop() {
  const std::uint32_t nowMs = millis();
  initializeStandaloneDisplay(nowMs);
  const bool connected = platform::connected();
  if (connected != helperConnected) {
    pendingDelay.reset();
    actionRuns.reset();
    remoteDisplay.emplace();
    platform::resetRemoteDisplay();
    displayStatus.setUsbConnected(connected);
    if (connected) {
      showStatus(LocalDisplayPriority::Startup);
      writeLine(helloLine);
    } else {
      applyDisplayUpdate(
          displayController.helperDisconnected(helperOfflineFrame()));
    }
  }
  helperConnected = connected;
  servicePendingDelay(nowMs);
  actionRuns.expire(nowMs);
  readHelperResponses(nowMs);
  if (controller.isLearning()) {
    scanLearningInputs(nowMs);
  } else {
    scanRuntimeInputs(nowMs);
  }
  platform::serviceDisplay();
  platform::delayMs(1);
}
