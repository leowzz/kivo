#pragma once

#include <cstddef>
#include <cstdint>
#include <optional>
#include <string>
#include <string_view>

#include "GpioTriggerController.h"

enum class HelperResponseKind {
  Paste,
  Hotkey,
  Skip,
};

struct HelperResponse {
  HelperResponseKind kind;
  std::uint32_t eventId;
  std::uint8_t modifierMask = 0;
  std::uint8_t keycode = 0;
};

class ResponseLineBuffer {
 public:
  explicit ResponseLineBuffer(std::size_t maxLength) : maxLength_(maxLength) {}

  std::optional<std::string> push(char character);

 private:
  std::size_t maxLength_;
  std::string line_;
  bool discardUntilNewline_ = false;
};

std::string formatPressEvent(const PressEvent &event);
std::optional<HelperResponse> parseHelperResponse(std::string_view line);
