#pragma once

#include <array>
#include <cstddef>
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
