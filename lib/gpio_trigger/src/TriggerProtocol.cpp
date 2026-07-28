#include "TriggerProtocol.h"

#include <limits>

namespace {
std::optional<std::uint32_t> parseNumber(std::string_view value) {
  if (value.empty()) return std::nullopt;
  std::uint32_t result = 0;
  for (const char character : value) {
    if (character < '0' || character > '9') return std::nullopt;
    const auto digit = static_cast<std::uint32_t>(character - '0');
    if (result > (std::numeric_limits<std::uint32_t>::max() - digit) / 10U)
      return std::nullopt;
    result = result * 10U + digit;
  }
  return result;
}

std::optional<std::string_view> takeToken(std::string_view &line) {
  while (!line.empty() && line.front() == ' ') line.remove_prefix(1);
  if (line.empty()) return std::nullopt;
  const auto separator = line.find(' ');
  const auto token = line.substr(0, separator);
  line = separator == std::string_view::npos ? std::string_view{}
                                             : line.substr(separator + 1);
  return token;
}
}  // namespace

std::optional<std::string> ResponseLineBuffer::push(char character) {
  if (discardUntilNewline_) {
    if (character == '\n') discardUntilNewline_ = false;
    return std::nullopt;
  }
  if (character == '\n') {
    line_.push_back(character);
    std::string complete;
    complete.swap(line_);
    return complete;
  }
  if (line_.size() < maxLength_) {
    line_.push_back(character);
  } else {
    line_.clear();
    discardUntilNewline_ = true;
  }
  return std::nullopt;
}

std::string formatPressEvent(const PressEvent &event) {
  return "PRESS " + std::to_string(event.id) + " " +
         std::to_string(event.gpio) + "\n";
}

std::optional<HelperResponse> parseHelperResponse(std::string_view line) {
  while (!line.empty() && (line.back() == '\n' || line.back() == '\r')) {
    line.remove_suffix(1);
  }
  const auto kindToken = takeToken(line);
  const auto eventToken = takeToken(line);
  if (!kindToken.has_value() || !eventToken.has_value()) return std::nullopt;
  const auto eventId = parseNumber(*eventToken);
  if (!eventId.has_value()) return std::nullopt;

  if (*kindToken == "PASTE" || *kindToken == "SKIP") {
    if (takeToken(line).has_value()) return std::nullopt;
    return HelperResponse{*kindToken == "PASTE" ? HelperResponseKind::Paste
                                                  : HelperResponseKind::Skip,
                          *eventId};
  }
  if (*kindToken != "HOTKEY") return std::nullopt;

  const auto maskToken = takeToken(line);
  const auto keyToken = takeToken(line);
  if (!maskToken.has_value() || !keyToken.has_value() ||
      takeToken(line).has_value()) {
    return std::nullopt;
  }
  const auto mask = parseNumber(*maskToken);
  const auto keycode = parseNumber(*keyToken);
  if (!mask.has_value() || *mask > 255 || !keycode.has_value() ||
      *keycode == 0 || *keycode > 164) {
    return std::nullopt;
  }
  return HelperResponse{HelperResponseKind::Hotkey, *eventId,
                        static_cast<std::uint8_t>(*mask),
                        static_cast<std::uint8_t>(*keycode)};
}
