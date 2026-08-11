#include "RemoteDisplay.h"

#include <algorithm>
#include <limits>
#include <utility>

#include "TriggerProtocol.h"

namespace {
bool isValidRect(const DisplayRect &bounds) {
  if (bounds.width == 0 || bounds.height == 0 || bounds.x % 8 != 0 ||
      bounds.y % 8 != 0 || bounds.width % 8 != 0 || bounds.height % 8 != 0) {
    return false;
  }
  return static_cast<std::uint32_t>(bounds.x) + bounds.width <=
             kRemoteDisplayWidth &&
         static_cast<std::uint32_t>(bounds.y) + bounds.height <=
             kRemoteDisplayHeight;
}

DisplayRect unionRect(const DisplayRect &left, const DisplayRect &right) {
  const auto x = std::min(left.x, right.x);
  const auto y = std::min(left.y, right.y);
  const auto rightEdge = std::max(
      static_cast<std::uint32_t>(left.x) + left.width,
      static_cast<std::uint32_t>(right.x) + right.width);
  const auto bottomEdge = std::max(
      static_cast<std::uint32_t>(left.y) + left.height,
      static_cast<std::uint32_t>(right.y) + right.height);
  return {x, y, static_cast<std::uint16_t>(rightEdge - x),
          static_cast<std::uint16_t>(bottomEdge - y)};
}

std::optional<std::size_t> findRegionIndex(const RemoteDisplayScene &scene,
                                           std::uint8_t slot) {
  for (std::size_t index = 0; index < scene.regionCount; ++index) {
    if (scene.regions[index].slot == slot) return index;
  }
  return std::nullopt;
}

int decodeBase64Character(char character) {
  if (character >= 'A' && character <= 'Z') return character - 'A';
  if (character >= 'a' && character <= 'z') return character - 'a' + 26;
  if (character >= '0' && character <= '9') return character - '0' + 52;
  if (character == '+') return 62;
  if (character == '/') return 63;
  return -1;
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

std::optional<std::uint32_t> parseRevision(std::string_view token) {
  if (token.empty()) return std::nullopt;
  std::uint32_t result = 0;
  for (const char character : token) {
    if (character < '0' || character > '9') return std::nullopt;
    const auto digit = static_cast<std::uint32_t>(character - '0');
    if (result > (std::numeric_limits<std::uint32_t>::max() - digit) / 10U) {
      return std::nullopt;
    }
    result = result * 10U + digit;
  }
  return result;
}

std::optional<std::string_view> displayErrorCode(std::string_view kind) {
  if (kind == "DISPLAY_BEGIN") return "invalid_begin";
  if (kind == "DISPLAY_REGION" || kind == "DISPLAY_CLEAR") {
    return "invalid_region";
  }
  if (kind == "DISPLAY_TEXT") return "invalid_text";
  if (kind == "DISPLAY_COMMIT") return "invalid_commit";
  if (kind.size() >= 8 && kind.substr(0, 8) == "DISPLAY_") {
    return "unsupported_display";
  }
  return std::nullopt;
}

bool isDisplayCommand(HelperCommandKind kind) {
  switch (kind) {
    case HelperCommandKind::DisplayBegin:
    case HelperCommandKind::DisplayRegion:
    case HelperCommandKind::DisplayClear:
    case HelperCommandKind::DisplayText:
    case HelperCommandKind::DisplayCommit:
      return true;
    default:
      return false;
  }
}
}  // namespace

DisplayResult RemoteDisplay::begin(std::uint32_t newRevision,
                                   std::uint32_t baseRevision,
                                   DisplayMode mode) {
  cancel();
  if (newRevision == 0 ||
      (mode == DisplayMode::Full && baseRevision != 0) ||
      (mode == DisplayMode::Delta &&
       (baseRevision == 0 || baseRevision == newRevision))) {
    return DisplayResult::Rejected;
  }
  if (mode == DisplayMode::Delta && baseRevision != revision_) {
    return DisplayResult::Resync;
  }
  staged_.emplace();
  staged_->revision = newRevision;
  staged_->mode = mode;
  return DisplayResult::Accepted;
}

bool RemoteDisplay::region(std::uint8_t slot, DisplayRect bounds) {
  if (!staged_.has_value() || !isValidRect(bounds) ||
      staged_->regionCount >= kMaxDisplayRegions ||
      findStagedRegion(slot) != nullptr) {
    return reject();
  }
  auto &regionState = staged_->regions[staged_->regionCount++];
  regionState.slot = slot;
  regionState.bounds = bounds;
  return true;
}

bool RemoteDisplay::clear(std::uint8_t slot) {
  if (findStagedRegion(slot) == nullptr ||
      staged_->operationCount >= kMaxDisplayOps) {
    return reject();
  }
  auto &operation = staged_->operations[staged_->operationCount++];
  operation.text.clear();
  operation.x = 0;
  operation.baselineY = 0;
  operation.slot = slot;
  operation.fontId = 0;
  operation.kind = DisplayOperationKind::Clear;
  return true;
}

bool RemoteDisplay::text(std::uint8_t slot, std::uint16_t x,
                         std::uint16_t baselineY, std::uint8_t fontId,
                         std::string_view value) {
  auto *regionState = findStagedRegion(slot);
  if (regionState == nullptr || staged_->operationCount >= kMaxDisplayOps ||
      value.size() > kMaxDisplayTextBytes ||
      fontId > kRemoteDisplayMaxFontId) {
    return reject();
  }
  const auto right = static_cast<std::uint32_t>(regionState->bounds.x) +
                     regionState->bounds.width;
  const auto bottom = static_cast<std::uint32_t>(regionState->bounds.y) +
                      regionState->bounds.height;
  if (x < regionState->bounds.x || x >= right ||
      baselineY < regionState->bounds.y || baselineY >= bottom) {
    return reject();
  }
  for (const unsigned char character : value) {
    if (character < 0x20 || character > 0x7e) return reject();
  }

  auto &operation = staged_->operations[staged_->operationCount++];
  operation.text.assign(value.data(), value.size());
  operation.x = x;
  operation.baselineY = baselineY;
  operation.slot = slot;
  operation.fontId = fontId;
  operation.kind = DisplayOperationKind::Text;
  return true;
}

const RemoteDisplayCommit *RemoteDisplay::commit(std::uint32_t revision) {
  if (!staged_.has_value() || staged_->revision != revision ||
      !buildCandidate()) {
    cancel();
    return nullptr;
  }

  lastCommit_.emplace();
  auto &result = *lastCommit_;
  static_cast<RemoteDisplayScene &>(result) = candidate_;
  result.full = staged_->mode == DisplayMode::Full;
  result.dirtyCount = 0;
  bool dirtyOverflow = false;
  for (std::size_t index = 0; index < staged_->regionCount; ++index) {
    const auto &changed = staged_->regions[index];
    DisplayRect dirty = changed.bounds;
    if (committed_.has_value()) {
      const auto previous = findRegionIndex(*committed_, changed.slot);
      if (previous.has_value()) {
        dirty = unionRect(committed_->regions[*previous].bounds, dirty);
      }
    }
    appendDirty(dirty, dirtyOverflow);
  }
  if (result.full && committed_.has_value()) {
    for (std::size_t oldIndex = 0; oldIndex < committed_->regionCount;
         ++oldIndex) {
      const auto oldSlot = committed_->regions[oldIndex].slot;
      if (!stagedContainsSlot(oldSlot)) {
        appendDirty(committed_->regions[oldIndex].bounds, dirtyOverflow);
      }
    }
  }

  revision_ = revision;
  committed_ = candidate_;
  cancel();
  return &result;
}

void RemoteDisplay::cancel() { staged_.reset(); }

std::optional<std::uint32_t> RemoteDisplay::stagedRevision() const {
  if (!staged_.has_value()) return std::nullopt;
  return staged_->revision;
}

DisplayRegionState *RemoteDisplay::findStagedRegion(std::uint8_t slot) {
  if (!staged_.has_value()) return nullptr;
  for (std::size_t index = 0; index < staged_->regionCount; ++index) {
    if (staged_->regions[index].slot == slot) return &staged_->regions[index];
  }
  return nullptr;
}

bool RemoteDisplay::stagedContainsSlot(std::uint8_t slot) const {
  if (!staged_.has_value()) return false;
  for (std::size_t index = 0; index < staged_->regionCount; ++index) {
    if (staged_->regions[index].slot == slot) return true;
  }
  return false;
}

bool RemoteDisplay::buildCandidate() {
  if (staged_->mode == DisplayMode::Full) {
    candidate_.revision = staged_->revision;
    candidate_.regionCount = staged_->regionCount;
    std::copy_n(staged_->regions.begin(), staged_->regionCount,
                candidate_.regions.begin());
    candidate_.operationCount = staged_->operationCount;
    std::copy_n(staged_->operations.begin(), staged_->operationCount,
                candidate_.operations.begin());
    return true;
  }
  if (!committed_.has_value()) return false;

  candidate_ = *committed_;
  candidate_.revision = staged_->revision;
  for (std::size_t index = 0; index < staged_->regionCount; ++index) {
    const auto &changed = staged_->regions[index];
    const auto existing = findRegionIndex(candidate_, changed.slot);
    if (existing.has_value()) {
      candidate_.regions[*existing] = changed;
    } else if (candidate_.regionCount < kMaxDisplayRegions) {
      candidate_.regions[candidate_.regionCount++] = changed;
    } else {
      return false;
    }
  }

  std::size_t retainedCount = 0;
  for (std::size_t index = 0; index < candidate_.operationCount; ++index) {
    if (stagedContainsSlot(candidate_.operations[index].slot)) continue;
    if (retainedCount != index) {
      candidate_.operations[retainedCount] =
          std::move(candidate_.operations[index]);
    }
    ++retainedCount;
  }
  if (retainedCount + staged_->operationCount > kMaxDisplayOps) return false;
  std::copy_n(staged_->operations.begin(), staged_->operationCount,
              candidate_.operations.begin() + retainedCount);
  candidate_.operationCount = retainedCount + staged_->operationCount;
  return true;
}

void RemoteDisplay::appendDirty(DisplayRect bounds, bool &overflowed) {
  if (overflowed) return;
  auto &result = *lastCommit_;
  if (result.dirtyCount < kMaxDisplayRegions) {
    result.dirtyBounds[result.dirtyCount++] = bounds;
  } else {
    result.dirtyBounds[0] =
        {0, 0, kRemoteDisplayWidth, kRemoteDisplayHeight};
    result.dirtyCount = 1;
    overflowed = true;
  }
}

bool RemoteDisplay::reject() {
  cancel();
  return false;
}

std::optional<std::string> decodeDisplayText(std::string_view encoded) {
  if (encoded.empty() || encoded.size() % 4 != 0 ||
      encoded.size() / 4 * 3 > kMaxDisplayTextBytes + 2) {
    return std::nullopt;
  }
  std::size_t padding = 0;
  if (encoded.back() == '=') ++padding;
  if (encoded.size() >= 2 && encoded[encoded.size() - 2] == '=') ++padding;
  const auto decodedLength = encoded.size() / 4 * 3 - padding;
  if (padding > 2 || decodedLength > kMaxDisplayTextBytes) {
    return std::nullopt;
  }

  std::string decoded;
  decoded.reserve(decodedLength);
  for (std::size_t offset = 0; offset < encoded.size(); offset += 4) {
    const bool finalGroup = offset + 4 == encoded.size();
    const char thirdCharacter = encoded[offset + 2];
    const char fourthCharacter = encoded[offset + 3];
    const int first = decodeBase64Character(encoded[offset]);
    const int second = decodeBase64Character(encoded[offset + 1]);
    const int third = thirdCharacter == '=' ? 0
                                             : decodeBase64Character(thirdCharacter);
    const int fourth = fourthCharacter == '='
                           ? 0
                           : decodeBase64Character(fourthCharacter);
    if (first < 0 || second < 0 || third < 0 || fourth < 0 ||
        (!finalGroup && (thirdCharacter == '=' || fourthCharacter == '=')) ||
        (thirdCharacter == '=' && fourthCharacter != '=') ||
        (finalGroup && padding == 0 &&
         (thirdCharacter == '=' || fourthCharacter == '=')) ||
        (finalGroup && padding == 1 && fourthCharacter != '=') ||
        (finalGroup && padding == 2 &&
         (thirdCharacter != '=' || fourthCharacter != '=')) ||
        (finalGroup && padding == 1 && (third & 0x03) != 0) ||
        (finalGroup && padding == 2 && (second & 0x0f) != 0)) {
      return std::nullopt;
    }
    const std::uint32_t value = static_cast<std::uint32_t>(first << 18) |
                                static_cast<std::uint32_t>(second << 12) |
                                static_cast<std::uint32_t>(third << 6) |
                                static_cast<std::uint32_t>(fourth);
    decoded.push_back(static_cast<char>((value >> 16) & 0xff));
    if (thirdCharacter != '=') {
      decoded.push_back(static_cast<char>((value >> 8) & 0xff));
    }
    if (fourthCharacter != '=') {
      decoded.push_back(static_cast<char>(value & 0xff));
    }
  }
  for (const unsigned char character : decoded) {
    if (character < 0x20 || character > 0x7e) return std::nullopt;
  }
  return decoded;
}

std::string formatDisplayOk(std::uint32_t revision) {
  return "DISPLAY_OK " + std::to_string(revision) + "\n";
}

std::string formatDisplayResync(std::uint32_t currentRevision) {
  return "DISPLAY_RESYNC " + std::to_string(currentRevision) + "\n";
}

std::string formatDisplayError(std::uint32_t revision, std::string_view code) {
  return "DISPLAY_ERROR " + std::to_string(revision) + " " +
         std::string(code) + "\n";
}

std::optional<std::string> dispatchDisplayCommand(RemoteDisplay &display,
                                                  const HelperCommand &command,
                                                  bool displaySupported) {
  if (!isDisplayCommand(command.kind)) return std::nullopt;
  const auto activeRevision = display.stagedRevision();
  const auto commandRevision =
      command.kind == HelperCommandKind::DisplayBegin ||
              command.kind == HelperCommandKind::DisplayCommit
          ? command.revision
          : activeRevision.value_or(0);
  if (!displaySupported) {
    display.cancel();
    return formatDisplayError(activeRevision.value_or(commandRevision),
                              "unsupported_display");
  }

  switch (command.kind) {
    case HelperCommandKind::DisplayBegin: {
      const auto result = display.begin(
          command.revision, command.baseRevision,
          command.displayFull ? DisplayMode::Full : DisplayMode::Delta);
      if (result == DisplayResult::Resync) {
        return formatDisplayResync(display.revision());
      }
      if (result == DisplayResult::Rejected) {
        return formatDisplayError(command.revision, "invalid_begin");
      }
      return std::nullopt;
    }
    case HelperCommandKind::DisplayRegion:
      if (!display.region(command.displaySlot,
                          {command.displayX, command.displayY,
                           command.displayWidth, command.displayHeight})) {
        return formatDisplayError(activeRevision.value_or(0),
                                  "invalid_region");
      }
      return std::nullopt;
    case HelperCommandKind::DisplayClear:
      if (!display.clear(command.displaySlot)) {
        return formatDisplayError(activeRevision.value_or(0),
                                  "invalid_region");
      }
      return std::nullopt;
    case HelperCommandKind::DisplayText:
      if (!display.text(command.displaySlot, command.displayX, command.displayY,
                        command.displayFontId, command.displayText)) {
        return formatDisplayError(activeRevision.value_or(0), "invalid_text");
      }
      return std::nullopt;
    case HelperCommandKind::DisplayCommit:
      if (display.commit(command.revision) == nullptr) {
        return formatDisplayError(activeRevision.value_or(command.revision),
                                  "invalid_commit");
      }
      return formatDisplayOk(command.revision);
    default:
      return std::nullopt;
  }
}

std::optional<std::string> discardMalformedDisplayCommand(
    RemoteDisplay &display, std::string_view line) {
  while (!line.empty() && (line.back() == '\n' || line.back() == '\r')) {
    line.remove_suffix(1);
  }
  const auto kind = takeToken(line);
  if (!kind.has_value()) return std::nullopt;
  const auto code = displayErrorCode(*kind);
  if (!code.has_value()) return std::nullopt;

  auto revision = display.stagedRevision();
  if (!revision.has_value() &&
      (*kind == "DISPLAY_BEGIN" || *kind == "DISPLAY_COMMIT")) {
    const auto token = takeToken(line);
    if (token.has_value()) revision = parseRevision(*token);
  }
  display.cancel();
  return formatDisplayError(revision.value_or(0), *code);
}
