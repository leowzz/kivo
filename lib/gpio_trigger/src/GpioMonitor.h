#pragma once

#include <string>

#include "BoardProfile.h"

// Passive sampling: the diagnostic request never changes pin modes or topology.
template <typename ReadPin>
std::string formatGpioState(const BoardProfile &board, ReadPin readPin) {
  std::string line = "GPIO_STATE " + std::to_string(board.safePinCount);
  for (std::size_t index = 0; index < board.safePinCount; ++index) {
    const auto pin = board.safePins[index];
    line += " " + std::to_string(pin) + (readPin(pin) ? ":1" : ":0");
  }
  return line + "\n";
}
