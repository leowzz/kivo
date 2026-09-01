#pragma once

#include <array>
#include <cstddef>
#include <cstdint>
#include <optional>
#include <string>

#include "GpioTriggerController.h"

enum class DisplayFrameLayout { Rows, UsageEmphasis };

struct DisplayFrame {
  std::array<std::string, 4> lines;
  DisplayFrameLayout layout = DisplayFrameLayout::Rows;

  bool operator==(const DisplayFrame &other) const {
    return lines == other.lines && layout == other.layout;
  }
};

inline std::uint8_t changedDisplayFrameLines(const DisplayFrame &previous,
                                             const DisplayFrame &next) {
  if (previous.layout != next.layout) return 0x0F;

  std::uint8_t changed = 0;
  for (std::size_t index = 0; index < next.lines.size(); ++index) {
    if (previous.lines[index] != next.lines[index]) {
      changed |= static_cast<std::uint8_t>(1U << index);
    }
  }
  return changed;
}

class DisplayStatusModel {
 public:
  void setUsbConnected(bool connected) { usbConnected_ = connected; }
  void setStandaloneDebug(bool enabled) { standaloneDebug_ = enabled; }
  void setReady(std::size_t keyCount);
  void setLearning(std::size_t pinCount);
  void setConfigError();
  void recordInput(const InputEvent &event);
  void clearLastInput();
  DisplayFrame frame() const;

 private:
  enum class Mode {
    Waiting,
    Ready,
    Learning,
    ConfigError,
  };

  bool usbConnected_ = false;
  bool standaloneDebug_ = false;
  Mode mode_ = Mode::Waiting;
  std::size_t count_ = 0;
  std::optional<PhysicalInput> lastInput_;
  InputState lastState_ = InputState::Up;
};
