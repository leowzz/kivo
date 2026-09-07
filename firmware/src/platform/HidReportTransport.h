#pragma once

#include <array>
#include <cstddef>
#include <cstdint>

namespace platform {
struct KeyboardReport {
  std::uint8_t modifiers = 0;
  std::array<std::uint8_t, 6> keys{};
};

template <typename Ready, typename Send, typename Pause>
bool transmitKeyboardReports(std::uint8_t modifiers,
                             const std::array<std::uint8_t, 6> &keys,
                             std::size_t readyPollLimit, Ready ready,
                             Send send, Pause pause) {
  const auto transmit = [&](const KeyboardReport &report) {
    for (std::size_t poll = 0; poll <= readyPollLimit; ++poll) {
      if (ready()) return send(report);
      if (poll == readyPollLimit) return false;
      pause();
    }
    return false;
  };

  return transmit(KeyboardReport{modifiers, keys}) && transmit(KeyboardReport{});
}

template <typename Ready, typename Send, typename Pause>
bool transmitHotkeyReports(std::uint8_t modifiers, std::uint8_t keycode,
                           std::size_t readyPollLimit, Ready ready, Send send,
                           Pause pause) {
  std::array<std::uint8_t, 6> keys{};
  keys[0] = keycode;
  return transmitKeyboardReports(
      modifiers, keys, readyPollLimit, ready,
      [&](const KeyboardReport &report) {
        return send(report.modifiers, report.keys[0]);
      },
      pause);
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
