#include <Arduino.h>

#include <algorithm>
#include <string>
#include <optional>
#include <vector>

#include "ActionRunController.h"
#include "ActionRunDispatcher.h"
#include "DisplayRuntime.h"
#include "display/U8g2Display.h"
#include "DisplayStatus.h"
#include "EmbeddedProduct.h"
#include "GpioTriggerController.h"
#include "GpioMonitor.h"
#include "Handshake.h"
#include "KeyActivityIndicator.h"
#include "ControlPanel.h"
#include "StandaloneDebugTopology.h"
#include "TriggerProtocol.h"
#include "platform/Platform.h"

namespace {
constexpr std::size_t kMaxResponseLineLength = 255;
constexpr std::size_t kProductChunkBytes = 144;
constexpr std::uint32_t kStandaloneDisplayStartupDelayMs = 500;
std::string helloLine;

GpioTriggerController controller(platform::boardProfile());
ActionRunController actionRuns;
KeyActivityIndicator keyIndicator;
ResponseLineBuffer responseLines(kMaxResponseLineLength);
TopologyBuilder topologyBuilder(platform::boardProfile());
DisplayStatusModel displayStatus;
U8g2Display displaySurface;
DisplayRuntime displayRuntime(displaySurface);
ControlPanel controls;
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

std::string encodeBase64(const std::uint8_t *data, std::size_t length) {
  static constexpr char alphabet[] =
      "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
  std::string encoded;
  encoded.reserve(((length + 2) / 3) * 4);
  for (std::size_t offset = 0; offset < length; offset += 3) {
    const std::size_t remaining = length - offset;
    const std::uint32_t value =
        (static_cast<std::uint32_t>(data[offset]) << 16) |
        (remaining > 1 ? static_cast<std::uint32_t>(data[offset + 1]) << 8
                       : 0) |
        (remaining > 2 ? static_cast<std::uint32_t>(data[offset + 2]) : 0);
    encoded += alphabet[(value >> 18) & 0x3f];
    encoded += alphabet[(value >> 12) & 0x3f];
    encoded += remaining > 1 ? alphabet[(value >> 6) & 0x3f] : '=';
    encoded += remaining > 2 ? alphabet[value & 0x3f] : '=';
  }
  return encoded;
}

void writeProductInfo() {
  if (kKivoProductDefinitionSize == 0) {
    writeLine("PRODUCT_INFO - 1 0 -\n");
    return;
  }
  writeLine("PRODUCT_INFO " + std::string(kKivoProductVersionId) + " 1 " +
            std::to_string(kKivoProductDefinitionSize) + " " +
            kKivoProductDefinitionSha256 + "\n");
}

void writeProductDefinition() {
  if (kKivoProductDefinitionSize == 0) {
    writeLine("PRODUCT_ERROR unavailable\n");
    return;
  }
  writeLine("PRODUCT_BEGIN " + std::to_string(kKivoProductDefinitionSize) +
            " " + kKivoProductDefinitionSha256 + "\n");
  std::size_t sequence = 0;
  for (std::size_t offset = 0; offset < kKivoProductDefinitionSize;
       offset += kProductChunkBytes, ++sequence) {
    const std::size_t length =
        std::min(kProductChunkBytes, kKivoProductDefinitionSize - offset);
    writeLine("PRODUCT_CHUNK " + std::to_string(sequence) + " " +
              encodeBase64(kKivoProductDefinition + offset, length) + "\n");
  }
  writeLine("PRODUCT_END " + std::to_string(kKivoProductDefinitionSize) + " " +
            kKivoProductDefinitionSha256 + "\n");
}

bool pasteClipboard() {
  return platform::sendHotkey(0x08, 0x19);
}

void showStatus(LocalDisplayPriority priority) {
  displayRuntime.showLocal(displayStatus.frame(), priority);
}

void showControlPanel() {
  displayRuntime.showInteractive(controls.frame(displayStatus.frame(), *controller.topology().display));
}

DisplayFrame helperOfflineFrame() {
  auto frame = displayStatus.frame();
  frame.lines[1] = "HELPER OFFLINE  ";
  frame.lines[2].clear();
  return frame;
}

bool isActiveDisplayPin(std::uint8_t pin) {
  const auto &display = controller.topology().display;
  return display.has_value() && (pin == display->sda || pin == display->scl);
}

void applyControlPanelPinModes() {
  if (const auto &panel = controller.topology().controls;
      panel.has_value()) {
    pinMode(panel->confirm, INPUT_PULLUP);
    pinMode(panel->encoderPress, INPUT_PULLUP);
    pinMode(panel->encoderA, INPUT_PULLUP);
    pinMode(panel->encoderB, INPUT_PULLUP);
    pinMode(panel->back, INPUT_PULLUP);
  }
}

void applyRuntimePinModes() {
  const auto &profile = platform::boardProfile();
  for (std::size_t index = 0; index < profile.safePinCount; ++index) {
    const auto pin = profile.safePins[index];
    if (!isActiveDisplayPin(pin)) pinMode(pin, INPUT);
  }
  for (const auto &source : controller.topology().directs) {
    for (const auto gpio : source.pins) pinMode(gpio, INPUT_PULLUP);
  }
  for (const auto &source : controller.topology().matrices) {
    for (const auto gpio : source.rows) pinMode(gpio, INPUT_PULLUP);
    for (const auto gpio : source.columns) pinMode(gpio, INPUT_PULLUP);
  }
  applyControlPanelPinModes();
}

void configError(std::uint32_t revision, const char *code) {
  controls.reset();
  displayStatus.setConfigError();
  showStatus(LocalDisplayPriority::Critical);
  writeLine("CONFIG_ERROR " + std::to_string(revision) + " " + code + "\n");
}

void resetKeyIndicator() {
  keyIndicator.reset();
  platform::clearKeyColor();
}

void applyTopologyState(const RuntimeTopology &topology, std::uint32_t nowMs) {
  controls.reset();
  displayRuntime.dismissInteractive();
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
  displayStatus.setReady(topology.keyCount());
  displayStatus.clearLastInput();
  // configure releases the previous display's bus before GPIO reassignment.
  const auto status = displayStatus.frame();
  displayRuntime.configure(topology.display, status, controls.brightnessPercent());
  applyTopologyState(topology, nowMs);
  showStatus(LocalDisplayPriority::Normal);
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
  displayRuntime.configure(controller.topology().display, displayStatus.frame(),
                           controls.brightnessPercent());
}

void handleResponseLine(std::string_view line, std::uint32_t nowMs) {
  const auto command = parseHelperCommand(line);
  if (!command.has_value()) {
    topologyBuilder.cancel();
    const auto displayError =
        displayRuntime.malformed(line);
    if (displayError.has_value()) writeLine(*displayError);
    return;
  }
  switch (command->kind) {
    case HelperCommandKind::DisplayCpu:
      controls.setCpuPercent(command->cpuPercent, nowMs);
      return;
    case HelperCommandKind::Hello:
      writeLine(helloLine);
      return;
    case HelperCommandKind::GpioRead:
      writeLine(formatGpioState(platform::boardProfile(), [](std::uint8_t pin) {
        return digitalRead(pin) == HIGH;
      }));
      return;
    case HelperCommandKind::ProductInfo:
      writeProductInfo();
      return;
    case HelperCommandKind::ProductRead:
      writeProductDefinition();
      return;
    case HelperCommandKind::ConfigBegin:
      pendingDelay.reset();
      actionRuns.reset();
      if (!topologyBuilder.begin(command->revision, command->debounceMs)) {
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
    case HelperCommandKind::ConfigDisplay:
      if (!topologyBuilder.addDisplay(command->revision,
              {command->displayPanel, command->displaySda, command->displayScl,
               command->displayAddress})) {
        topologyBuilder.cancel();
        configError(command->revision, "invalid_display");
      }
      return;
    case HelperCommandKind::ConfigControls:
      if (!topologyBuilder.addControlPanel(
              command->revision, command->pins[0], command->pins[1],
              command->pins[2], command->pins[3], command->pins[4])) {
        topologyBuilder.cancel();
        configError(command->revision, "invalid_controls");
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
      const auto reply = displayRuntime.handle(*command);
      if (reply.has_value()) writeLine(*reply);
      return;
    }
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
          displayRuntime.malformed(line->line);
      if (displayError.has_value()) writeLine(*displayError);
      continue;
    }
    handleResponseLine(line->line, nowMs);
  }
}

void resetHelperInput() {
  responseLines = ResponseLineBuffer(kMaxResponseLineLength);
  while (platform::available() > 0) {
    if (platform::read() < 0) return;
  }
}

void emitInput(const std::optional<InputEvent> &event) {
  if (event.has_value()) {
    displayStatus.recordInput(*event);
    if (controls.active()) {
      showControlPanel();
    } else {
      showStatus(LocalDisplayPriority::Normal);
    }
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
    writeLine(formatInputEvent(*event));
  }
}

void scanControlPanel(std::uint32_t nowMs) {
  const auto &panel = controller.topology().controls;
  if (!panel.has_value()) return;
  const ControlPanelSample sample{
      digitalRead(panel->confirm) == LOW,
      digitalRead(panel->encoderPress) == LOW,
      digitalRead(panel->encoderA) == HIGH,
      digitalRead(panel->encoderB) == HIGH,
      digitalRead(panel->back) == LOW,
  };
  const auto update = controls.update(
      sample, nowMs, controller.topology().debounceMs);
  switch (update) {
    case ControlPanelUpdate::Render:
      showControlPanel();
      break;
    case ControlPanelUpdate::Dismiss:
      displayRuntime.dismissInteractive();
      break;
    case ControlPanelUpdate::BrightnessChanged:
      displayRuntime.setBrightness(controls.brightnessPercent());
      platform::saveDisplayBrightness(controls.brightnessPercent());
      showControlPanel();
      break;
    case ControlPanelUpdate::AnimationChanged:
      platform::saveDisplayAnimation(controls.animation());
      showControlPanel();
      break;
    case ControlPanelUpdate::None:
      break;
  }
}

void scanRuntimeInputs(std::uint32_t nowMs) {
  for (const auto &source : controller.topology().directs) {
    for (const auto gpio : source.pins) {
      emitInput(controller.updatePin(gpio, digitalRead(gpio) == HIGH, nowMs));
    }
  }

  for (const auto &source : controller.topology().matrices) {
    for (const auto row : source.rows) {
      pinMode(row, OUTPUT);
      digitalWrite(row, LOW);
      delayMicroseconds(5);
      for (const auto column : source.columns) {
        emitInput(controller.updateContact(source.index, row, column,
                                           digitalRead(column) == LOW, nowMs));
      }
      pinMode(row, INPUT_PULLUP);
    }
  }
}

}  // namespace

void setup() {
  helloLine = formatHello(platform::boardProfile(), KIVO_FIRMWARE_BUILD_ID,
                          kKivoProductVersionId);
  platform::begin();
  controls.setBrightnessPercent(platform::loadDisplayBrightness());
  controls.setAnimation(platform::loadDisplayAnimation());
  const auto productTopology =
      makeEmbeddedProductTopology(platform::boardProfile());
  if (productTopology.has_value()) {
    activateTopology(*productTopology, millis());
    return;
  }
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
    controls.clearCpuPercent();
    pendingDelay.reset();
    actionRuns.reset();
    resetHelperInput();
    displayStatus.setUsbConnected(connected);
    displayRuntime.connectionChanged(connected,
        connected ? displayStatus.frame() : helperOfflineFrame());
    if (connected) writeLine(helloLine);
    if (controls.active()) showControlPanel();
  }
  helperConnected = connected;
  servicePendingDelay(nowMs);
  actionRuns.expire(nowMs);
  if (helperConnected) readHelperResponses(nowMs);
  scanRuntimeInputs(nowMs);
  scanControlPanel(nowMs);
  if (controls.tickAnimation(nowMs)) showControlPanel();
  if (const auto error = displayRuntime.service(); error && helperConnected) {
    writeLine(*error);
  }
  platform::delayMs(1);
}
