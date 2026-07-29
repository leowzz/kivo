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
  ConfigBegin,
  ConfigDirect,
  ConfigMatrix,
  ConfigCommit,
  LearnBegin,
  LearnEnd,
  Paste,
  Hotkey,
  Skip,
};

struct HelperCommand {
  HelperCommandKind kind;
  std::uint32_t revision = 0;
  std::uint32_t eventId = 0;
  std::uint16_t debounceMs = 0;
  std::uint16_t step = 0;
  std::uint16_t total = 0;
  std::uint8_t sourceIndex = 0;
  std::uint8_t modifierMask = 0;
  std::uint8_t keycode = 0;
  std::vector<std::uint8_t> pins;
  std::vector<std::uint8_t> rows;
  std::vector<std::uint8_t> columns;
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

std::string formatInputEvent(const InputEvent &event);
std::string formatLearningEvent(const InputEvent &event);
std::string formatDone(std::uint32_t eventId, std::uint16_t step);
std::optional<HelperCommand> parseHelperCommand(std::string_view line);
