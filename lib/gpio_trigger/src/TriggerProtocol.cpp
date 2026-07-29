#include "TriggerProtocol.h"

#include <algorithm>
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

std::optional<std::uint32_t> takeNumber(std::string_view &line) {
  const auto token = takeToken(line);
  return token.has_value() ? parseNumber(*token) : std::nullopt;
}

bool takePins(std::string_view &line, std::size_t count,
              std::vector<std::uint8_t> &pins) {
  if (count == 0 || count > kEsp32S3SafePins.size()) return false;
  for (std::size_t index = 0; index < count; ++index) {
    const auto value = takeNumber(line);
    if (!value.has_value() || *value > 255) return false;
    const auto pin = static_cast<std::uint8_t>(*value);
    if (std::find(kEsp32S3SafePins.begin(), kEsp32S3SafePins.end(), pin) ==
            kEsp32S3SafePins.end() ||
        std::find(pins.begin(), pins.end(), pin) != pins.end()) {
      return false;
    }
    pins.push_back(pin);
  }
  return true;
}

void trimLineEnd(std::string_view &line) {
  while (!line.empty() && (line.back() == '\n' || line.back() == '\r')) {
    line.remove_suffix(1);
  }
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

std::string formatInputEvent(const InputEvent &event) {
  std::string result = "STATE " + std::to_string(event.id) + " ";
  if (event.input.kind == PhysicalInputKind::Direct) {
    result += "DIRECT " + std::to_string(event.input.pinA);
  } else {
    result += "CONTACT " + std::to_string(event.input.sourceIndex) + " " +
              std::to_string(event.input.pinA) + " " +
              std::to_string(event.input.pinB);
  }
  return result + (event.state == InputState::Down ? " DOWN\n" : " UP\n");
}

std::string formatLearningEvent(const InputEvent &event) {
  std::string result = event.input.kind == PhysicalInputKind::Direct
                           ? "LEARN_DIRECT " + std::to_string(event.input.pinA)
                           : "LEARN_CONTACT " +
                                 std::to_string(event.input.pinA) + " " +
                                 std::to_string(event.input.pinB);
  return result + (event.state == InputState::Down ? " DOWN\n" : " UP\n");
}

std::string formatDone(std::uint32_t eventId, std::uint16_t step) {
  return "DONE " + std::to_string(eventId) + " " + std::to_string(step) +
         "\n";
}

std::optional<HelperCommand> parseHelperCommand(std::string_view line) {
  if (line.size() > 255) return std::nullopt;
  trimLineEnd(line);
  const auto kind = takeToken(line);
  if (!kind.has_value()) return std::nullopt;

  if (*kind == "HELLO") {
    return takeToken(line).has_value()
               ? std::nullopt
               : std::optional<HelperCommand>{{HelperCommandKind::Hello}};
  }

  if (*kind == "CONFIG_BEGIN") {
    const auto revision = takeNumber(line);
    const auto debounce = takeNumber(line);
    if (!revision.has_value() || !debounce.has_value() || *debounce == 0 ||
        *debounce > 1000 || takeToken(line).has_value()) {
      return std::nullopt;
    }
    HelperCommand command{HelperCommandKind::ConfigBegin};
    command.revision = *revision;
    command.debounceMs = static_cast<std::uint16_t>(*debounce);
    return command;
  }

  if (*kind == "CONFIG_DIRECT") {
    const auto revision = takeNumber(line);
    const auto source = takeNumber(line);
    const auto count = takeNumber(line);
    if (!revision.has_value() || !source.has_value() || *source > 255 ||
        !count.has_value()) {
      return std::nullopt;
    }
    HelperCommand command{HelperCommandKind::ConfigDirect};
    command.revision = *revision;
    command.sourceIndex = static_cast<std::uint8_t>(*source);
    if (!takePins(line, *count, command.pins) || takeToken(line).has_value()) {
      return std::nullopt;
    }
    return command;
  }

  if (*kind == "CONFIG_MATRIX") {
    const auto revision = takeNumber(line);
    const auto source = takeNumber(line);
    const auto rowCount = takeNumber(line);
    if (!revision.has_value() || !source.has_value() || *source > 255 ||
        !rowCount.has_value()) {
      return std::nullopt;
    }
    HelperCommand command{HelperCommandKind::ConfigMatrix};
    command.revision = *revision;
    command.sourceIndex = static_cast<std::uint8_t>(*source);
    if (!takePins(line, *rowCount, command.rows)) return std::nullopt;
    const auto columnCount = takeNumber(line);
    command.columns = command.rows;
    if (!columnCount.has_value() ||
        !takePins(line, *columnCount, command.columns) ||
        takeToken(line).has_value()) {
      return std::nullopt;
    }
    command.columns.erase(command.columns.begin(),
                          command.columns.begin() + command.rows.size());
    return command;
  }

  if (*kind == "CONFIG_COMMIT" || *kind == "LEARN_END") {
    const auto revision = takeNumber(line);
    if (!revision.has_value() || takeToken(line).has_value()) return std::nullopt;
    HelperCommand command{*kind == "CONFIG_COMMIT"
                              ? HelperCommandKind::ConfigCommit
                              : HelperCommandKind::LearnEnd};
    command.revision = *revision;
    return command;
  }

  if (*kind == "LEARN_BEGIN") {
    const auto revision = takeNumber(line);
    const auto count = takeNumber(line);
    if (!revision.has_value() || !count.has_value()) return std::nullopt;
    HelperCommand command{HelperCommandKind::LearnBegin};
    command.revision = *revision;
    if (!takePins(line, *count, command.pins) || takeToken(line).has_value()) {
      return std::nullopt;
    }
    return command;
  }

  const auto eventId = takeNumber(line);
  if (!eventId.has_value() || *eventId == 0) return std::nullopt;
  if (*kind == "SKIP") {
    if (takeToken(line).has_value()) return std::nullopt;
    HelperCommand command{HelperCommandKind::Skip};
    command.eventId = *eventId;
    return command;
  }
  if (*kind != "PASTE" && *kind != "HOTKEY") return std::nullopt;
  const auto step = takeNumber(line);
  const auto total = takeNumber(line);
  if (!step.has_value() || !total.has_value() || *step == 0 || *total == 0 ||
      *step > *total || *total > std::numeric_limits<std::uint16_t>::max()) {
    return std::nullopt;
  }
  HelperCommand command{*kind == "PASTE" ? HelperCommandKind::Paste
                                          : HelperCommandKind::Hotkey};
  command.eventId = *eventId;
  command.step = static_cast<std::uint16_t>(*step);
  command.total = static_cast<std::uint16_t>(*total);
  if (*kind == "PASTE") {
    return takeToken(line).has_value() ? std::nullopt
                                       : std::optional<HelperCommand>{command};
  }
  const auto mask = takeNumber(line);
  const auto keycode = takeNumber(line);
  if (!mask.has_value() || *mask > 255 || !keycode.has_value() ||
      *keycode == 0 || *keycode > 164 || takeToken(line).has_value()) {
    return std::nullopt;
  }
  command.modifierMask = static_cast<std::uint8_t>(*mask);
  command.keycode = static_cast<std::uint8_t>(*keycode);
  return command;
}
