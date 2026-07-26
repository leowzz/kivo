#include "TriggerProtocol.h"

#include <limits>

std::string formatPressEvent(const PressEvent &event) {
  return "PRESS " + std::to_string(event.id) + " " +
         std::to_string(event.gpio) + "\n";
}

std::optional<HelperResponse> parseHelperResponse(std::string_view line) {
  if (!line.empty() && line.back() == '\n') {
    line.remove_suffix(1);
  }
  if (!line.empty() && line.back() == '\r') {
    line.remove_suffix(1);
  }

  HelperResponseKind kind;
  if (line.rfind("PASTE ", 0) == 0) {
    kind = HelperResponseKind::Paste;
    line.remove_prefix(6);
  } else if (line.rfind("SKIP ", 0) == 0) {
    kind = HelperResponseKind::Skip;
    line.remove_prefix(5);
  } else {
    return std::nullopt;
  }

  if (line.empty()) {
    return std::nullopt;
  }

  std::uint32_t eventId = 0;
  for (const char character : line) {
    if (character < '0' || character > '9') {
      return std::nullopt;
    }
    const std::uint32_t digit = static_cast<std::uint32_t>(character - '0');
    if (eventId >
        (std::numeric_limits<std::uint32_t>::max() - digit) / 10U) {
      return std::nullopt;
    }
    eventId = eventId * 10U + digit;
  }

  return HelperResponse{kind, eventId};
}
