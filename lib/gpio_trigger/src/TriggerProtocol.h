#pragma once

#include <cstdint>
#include <optional>
#include <string>
#include <string_view>

#include "GpioTriggerController.h"

enum class HelperResponseKind {
  Paste,
  Skip,
};

struct HelperResponse {
  HelperResponseKind kind;
  std::uint32_t eventId;
};

std::string formatPressEvent(const PressEvent &event);
std::optional<HelperResponse> parseHelperResponse(std::string_view line);
