#pragma once

#include <cstddef>
#include <cstdint>

namespace platform {
template <typename Ready, typename Send, typename Pause>
bool transmitHotkeyReports(std::uint8_t modifiers, std::uint8_t keycode,
                           std::size_t readyPollLimit, Ready ready, Send send,
                           Pause pause) {
  const auto transmit = [&](std::uint8_t reportModifiers,
                            std::uint8_t reportKeycode) {
    for (std::size_t poll = 0; poll <= readyPollLimit; ++poll) {
      if (ready()) return send(reportModifiers, reportKeycode);
      if (poll == readyPollLimit) return false;
      pause();
    }
    return false;
  };

  return transmit(modifiers, keycode) && transmit(0, 0);
}

template <typename Ready, typename Send, typename Pause>
bool transmitConsumerReports(std::uint16_t usage, std::size_t readyPollLimit,
                             Ready ready, Send send, Pause pause) {
  const auto transmit = [&](std::uint16_t reportUsage) {
    for (std::size_t poll = 0; poll <= readyPollLimit; ++poll) {
      if (ready()) return send(reportUsage);
      if (poll == readyPollLimit) return false;
      pause();
    }
    return false;
  };

  return transmit(usage) && transmit(0);
}
}  // namespace platform
