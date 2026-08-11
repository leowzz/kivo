#pragma once

#include <array>
#include <cstddef>
#include <cstdint>

#include "ActionRunController.h"
#include "TriggerProtocol.h"

template <typename SendKeyboardChord, typename Complete>
bool executeKeyboardChord(ActionRunController &runs,
                          const HelperCommand &command,
                          std::uint32_t nowMs,
                          SendKeyboardChord sendKeyboardChord,
                          Complete complete) {
  if (runs.acceptStep(command.runId, command.step, command.total, nowMs) !=
      ResponseAction::Execute) {
    return false;
  }

  std::array<std::uint8_t, 6> keys{};
  for (std::size_t index = 0; index < command.keycodes.size(); ++index) {
    keys[index] = command.keycodes[index];
  }
  if (!sendKeyboardChord(command.modifierMask, keys)) return false;
  complete(command.runId, command.step);
  return true;
}
