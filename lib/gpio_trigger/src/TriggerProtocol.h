#pragma once

#include <cstddef>
#include <cstdint>
#include <optional>
#include <string>
#include <string_view>
#include <vector>

#include "GpioTriggerController.h"

enum class HelperCommandKind {
  Hello,
  ProductInfo,
  ProductRead,
  ConfigBegin,
  ConfigDirect,
  ConfigMatrix,
  ConfigOled,
  ConfigSh1106,
  ConfigOledControl,
  ConfigCommit,
  DisplayBegin,
  DisplayRegion,
  DisplayClear,
  DisplayText,
  DisplayCommit,
  LearnBegin,
  LearnEnd,
  Paste,
  Hotkey,
  Chord,
  Delay,
  Media,
  Host,
  Skip,
};

struct HelperCommand {
  HelperCommandKind kind;
  std::uint32_t revision = 0;
  std::uint32_t baseRevision = 0;
  std::uint32_t runId = 0;
  std::uint16_t debounceMs = 0;
  std::uint16_t step = 0;
  std::uint16_t total = 0;
  std::uint8_t sourceIndex = 0;
  std::uint8_t modifierMask = 0;
  std::uint8_t keycode = 0;
  std::vector<std::uint8_t> keycodes;
  std::uint16_t consumerUsage = 0;
  std::uint32_t durationMs = 0;
  std::uint8_t oledSda = 0;
  std::uint8_t oledScl = 0;
  bool displayFull = false;
  std::uint8_t displaySlot = 0;
  std::uint16_t displayX = 0;
  std::uint16_t displayY = 0;
  std::uint16_t displayWidth = 0;
  std::uint16_t displayHeight = 0;
  std::uint8_t displayFontId = 0;
  std::string displayText;
  std::vector<std::uint8_t> pins;
  std::vector<std::uint8_t> rows;
  std::vector<std::uint8_t> columns;
};

struct ResponseLineEvent {
  std::string line;
  bool overflow = false;
};

class ResponseLineBuffer {
 public:
  explicit ResponseLineBuffer(std::size_t maxLength) : maxLength_(maxLength) {}

  std::optional<ResponseLineEvent> push(char character);

 private:
  std::size_t maxLength_;
  std::string line_;
  bool discardUntilNewline_ = false;
};

std::string formatInputEvent(const InputEvent &event);
std::string formatLearningEvent(const InputEvent &event);
std::string formatDone(std::uint32_t runId, std::uint16_t step);
std::optional<HelperCommand> parseHelperCommand(std::string_view line);
