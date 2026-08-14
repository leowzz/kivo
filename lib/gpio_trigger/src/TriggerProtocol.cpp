#include "TriggerProtocol.h"

#include <algorithm>
#include <limits>
#include <utility>

#include "RemoteDisplay.h"

namespace {
constexpr std::size_t kMaxProtocolPinCount = 23;
constexpr std::size_t kMaxChordKeyCount = 6;
constexpr std::uint32_t kMaxDelayMs = 60000;

bool isSupportedConsumerUsage(std::uint32_t usage) {
  switch (usage) {
    case 0x00B5:
    case 0x00B6:
    case 0x00B7:
    case 0x00CD:
    case 0x00E2:
    case 0x00E9:
    case 0x00EA:
      return true;
    default:
      return false;
  }
}

bool isSupportedKeyboardUsage(std::uint32_t usage) {
  return (usage >= 0x04 && usage <= 0x31) ||
         (usage >= 0x33 && usage <= 0x63) || usage == 0x65 ||
         (usage >= 0x67 && usage <= 0x73);
}

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
  if (count == 0 || count > kMaxProtocolPinCount) return false;
  for (std::size_t index = 0; index < count; ++index) {
    const auto value = takeNumber(line);
    if (!value.has_value() ||
        *value > std::numeric_limits<std::uint8_t>::max()) {
      return false;
    }
    const auto pin = static_cast<std::uint8_t>(*value);
    if (std::find(pins.begin(), pins.end(), pin) != pins.end()) {
      return false;
    }
    pins.push_back(pin);
  }
  return true;
}

bool takeChordKeycodes(std::string_view &line, std::size_t count,
                       std::vector<std::uint8_t> &keycodes) {
  if (count > kMaxChordKeyCount) return false;
  for (std::size_t index = 0; index < count; ++index) {
    const auto value = takeNumber(line);
    if (!value.has_value() || !isSupportedKeyboardUsage(*value)) return false;
    const auto keycode = static_cast<std::uint8_t>(*value);
    if (std::find(keycodes.begin(), keycodes.end(), keycode) != keycodes.end()) {
      return false;
    }
    keycodes.push_back(keycode);
  }
  return true;
}

void trimLineEnd(std::string_view &line) {
  while (!line.empty() && (line.back() == '\n' || line.back() == '\r')) {
    line.remove_suffix(1);
  }
}
}  // namespace

std::optional<ResponseLineEvent> ResponseLineBuffer::push(char character) {
  if (discardUntilNewline_) {
    if (character == '\n') {
      discardUntilNewline_ = false;
      std::string prefix;
      prefix.swap(line_);
      return ResponseLineEvent{std::move(prefix), true};
    }
    return std::nullopt;
  }
  if (character == '\n') {
    line_.push_back(character);
    std::string complete;
    complete.swap(line_);
    return ResponseLineEvent{std::move(complete), false};
  }
  if (line_.size() < maxLength_) {
    line_.push_back(character);
  } else {
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

std::string formatDone(std::uint32_t runId, std::uint16_t step) {
  return "DONE " + std::to_string(runId) + " " + std::to_string(step) +
         "\n";
}

std::optional<HelperCommand> parseHelperCommand(std::string_view line) {
  if (line.size() >= 255) return std::nullopt;
  trimLineEnd(line);
  const auto kind = takeToken(line);
  if (!kind.has_value()) return std::nullopt;

  if (*kind == "HELLO") {
    return takeToken(line).has_value()
               ? std::nullopt
               : std::optional<HelperCommand>{{HelperCommandKind::Hello}};
  }

  if (*kind == "PRODUCT_INFO" || *kind == "PRODUCT_READ") {
    if (takeToken(line).has_value()) return std::nullopt;
    return HelperCommand{*kind == "PRODUCT_INFO"
                             ? HelperCommandKind::ProductInfo
                             : HelperCommandKind::ProductRead};
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

  if (*kind == "CONFIG_OLED") {
    const auto revision = takeNumber(line);
    const auto sda = takeNumber(line);
    const auto scl = takeNumber(line);
    if (!revision.has_value() || !sda.has_value() || *sda > 255 ||
        !scl.has_value() || *scl > 255 || takeToken(line).has_value()) {
      return std::nullopt;
    }
    HelperCommand command{HelperCommandKind::ConfigOled};
    command.revision = *revision;
    command.oledSda = static_cast<std::uint8_t>(*sda);
    command.oledScl = static_cast<std::uint8_t>(*scl);
    return command;
  }

  if (*kind == "CONFIG_OLED_CONTROL") {
    const auto revision = takeNumber(line);
    if (!revision.has_value()) return std::nullopt;
    HelperCommand command{HelperCommandKind::ConfigOledControl};
    command.revision = *revision;
    if (!takePins(line, 5, command.pins) || takeToken(line).has_value()) {
      return std::nullopt;
    }
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

  if (*kind == "DISPLAY_BEGIN") {
    const auto revision = takeNumber(line);
    const auto baseRevision = takeNumber(line);
    const auto mode = takeToken(line);
    if (!revision.has_value() || !baseRevision.has_value() ||
        !mode.has_value() || (*mode != "full" && *mode != "delta") ||
        takeToken(line).has_value()) {
      return std::nullopt;
    }
    HelperCommand command{HelperCommandKind::DisplayBegin};
    command.revision = *revision;
    command.baseRevision = *baseRevision;
    command.displayFull = *mode == "full";
    return command;
  }

  if (*kind == "DISPLAY_REGION") {
    const auto slot = takeNumber(line);
    const auto x = takeNumber(line);
    const auto y = takeNumber(line);
    const auto width = takeNumber(line);
    const auto height = takeNumber(line);
    if (!slot.has_value() || *slot > std::numeric_limits<std::uint8_t>::max() ||
        !x.has_value() || *x > std::numeric_limits<std::uint16_t>::max() ||
        !y.has_value() || *y > std::numeric_limits<std::uint16_t>::max() ||
        !width.has_value() ||
        *width > std::numeric_limits<std::uint16_t>::max() ||
        !height.has_value() ||
        *height > std::numeric_limits<std::uint16_t>::max() ||
        takeToken(line).has_value()) {
      return std::nullopt;
    }
    HelperCommand command{HelperCommandKind::DisplayRegion};
    command.displaySlot = static_cast<std::uint8_t>(*slot);
    command.displayX = static_cast<std::uint16_t>(*x);
    command.displayY = static_cast<std::uint16_t>(*y);
    command.displayWidth = static_cast<std::uint16_t>(*width);
    command.displayHeight = static_cast<std::uint16_t>(*height);
    return command;
  }

  if (*kind == "DISPLAY_CLEAR") {
    const auto slot = takeNumber(line);
    if (!slot.has_value() || *slot > std::numeric_limits<std::uint8_t>::max() ||
        takeToken(line).has_value()) {
      return std::nullopt;
    }
    HelperCommand command{HelperCommandKind::DisplayClear};
    command.displaySlot = static_cast<std::uint8_t>(*slot);
    return command;
  }

  if (*kind == "DISPLAY_TEXT") {
    const auto slot = takeNumber(line);
    const auto x = takeNumber(line);
    const auto baselineY = takeNumber(line);
    const auto fontId = takeNumber(line);
    const auto encoded = takeToken(line);
    if (!slot.has_value() || *slot > std::numeric_limits<std::uint8_t>::max() ||
        !x.has_value() || *x > std::numeric_limits<std::uint16_t>::max() ||
        !baselineY.has_value() ||
        *baselineY > std::numeric_limits<std::uint16_t>::max() ||
        !fontId.has_value() ||
        *fontId > std::numeric_limits<std::uint8_t>::max() ||
        !encoded.has_value() || takeToken(line).has_value()) {
      return std::nullopt;
    }
    const auto decoded = decodeDisplayText(*encoded);
    if (!decoded.has_value()) return std::nullopt;
    HelperCommand command{HelperCommandKind::DisplayText};
    command.displaySlot = static_cast<std::uint8_t>(*slot);
    command.displayX = static_cast<std::uint16_t>(*x);
    command.displayY = static_cast<std::uint16_t>(*baselineY);
    command.displayFontId = static_cast<std::uint8_t>(*fontId);
    command.displayText = *decoded;
    return command;
  }

  if (*kind == "DISPLAY_COMMIT") {
    const auto revision = takeNumber(line);
    if (!revision.has_value() || takeToken(line).has_value()) {
      return std::nullopt;
    }
    HelperCommand command{HelperCommandKind::DisplayCommit};
    command.revision = *revision;
    return command;
  }

  const auto runId = takeNumber(line);
  if (!runId.has_value() || *runId == 0) return std::nullopt;
  if (*kind == "SKIP") {
    if (takeToken(line).has_value()) return std::nullopt;
    HelperCommand command{HelperCommandKind::Skip};
    command.runId = *runId;
    return command;
  }
  if (*kind != "PASTE" && *kind != "HOTKEY" && *kind != "DELAY" &&
      *kind != "MEDIA" && *kind != "HOST" && *kind != "CHORD") {
    return std::nullopt;
  }
  const auto step = takeNumber(line);
  const auto total = takeNumber(line);
  if (!step.has_value() || !total.has_value() || *step == 0 || *total == 0 ||
      *step > *total || *total > std::numeric_limits<std::uint16_t>::max()) {
    return std::nullopt;
  }
  HelperCommandKind commandKind = HelperCommandKind::Host;
  if (*kind == "PASTE") commandKind = HelperCommandKind::Paste;
  if (*kind == "HOTKEY") commandKind = HelperCommandKind::Hotkey;
  if (*kind == "CHORD") commandKind = HelperCommandKind::Chord;
  if (*kind == "DELAY") commandKind = HelperCommandKind::Delay;
  if (*kind == "MEDIA") commandKind = HelperCommandKind::Media;
  HelperCommand command{commandKind};
  command.runId = *runId;
  command.step = static_cast<std::uint16_t>(*step);
  command.total = static_cast<std::uint16_t>(*total);
  if (*kind == "PASTE" || *kind == "HOST") {
    return takeToken(line).has_value() ? std::nullopt
                                       : std::optional<HelperCommand>{command};
  }
  if (*kind == "DELAY") {
    const auto duration = takeNumber(line);
    if (!duration.has_value() || *duration == 0 || *duration > kMaxDelayMs ||
        takeToken(line).has_value()) {
      return std::nullopt;
    }
    command.durationMs = *duration;
    return command;
  }
  if (*kind == "MEDIA") {
    const auto usage = takeNumber(line);
    if (!usage.has_value() || !isSupportedConsumerUsage(*usage) ||
        takeToken(line).has_value()) {
      return std::nullopt;
    }
    command.consumerUsage = static_cast<std::uint16_t>(*usage);
    return command;
  }
  if (*kind == "CHORD") {
    const auto mask = takeNumber(line);
    const auto keyCount = takeNumber(line);
    if (!mask.has_value() || *mask > 255 || !keyCount.has_value() ||
        *keyCount > kMaxChordKeyCount ||
        (*mask == 0 && *keyCount == 0) ||
        !takeChordKeycodes(line, *keyCount, command.keycodes) ||
        takeToken(line).has_value()) {
      return std::nullopt;
    }
    command.modifierMask = static_cast<std::uint8_t>(*mask);
    return command;
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
