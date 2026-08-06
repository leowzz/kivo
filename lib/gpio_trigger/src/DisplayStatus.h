#pragma once

#include <array>
#include <cstddef>
#include <optional>
#include <string>

#include "GpioTriggerController.h"

struct DisplayFrame {
  std::array<std::string, 3> lines;

  bool operator==(const DisplayFrame &other) const {
    return lines == other.lines;
  }
};

class DisplayStatusModel {
 public:
  void setUsbConnected(bool connected) { usbConnected_ = connected; }
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
  Mode mode_ = Mode::Waiting;
  std::size_t count_ = 0;
  std::optional<PhysicalInput> lastInput_;
  InputState lastState_ = InputState::Up;
};
