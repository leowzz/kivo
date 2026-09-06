#pragma once

#include <string>
#include <string_view>

#include "BoardProfile.h"
#include "GpioMonitor.h"
#include "Handshake.h"

enum class IoInputMode { Floating, PullUp, PullDown };

class IoTestProtocol {
 public:
  explicit IoTestProtocol(const BoardProfile &board) : board_(board) {}

  template <typename ConfigurePin>
  void reset(ConfigurePin configure) {
    apply(IoInputMode::Floating, configure);
  }

  template <typename ReadPin, typename ConfigurePin>
  std::string handle(std::string_view command, std::string_view buildId,
                     ReadPin read, ConfigurePin configure) {
    if (command == "HELLO") return formatHello(board_, buildId);
    if (command == "GPIO_READ") return formatGpioState(board_, read);
    if (command == "GPIO_MODE INPUT") apply(IoInputMode::Floating, configure);
    else if (command == "GPIO_MODE PULLUP") apply(IoInputMode::PullUp, configure);
    else if (command == "GPIO_MODE PULLDOWN") apply(IoInputMode::PullDown, configure);
    else if (command != "GPIO_MODE") return {};
    switch (mode_) {
      case IoInputMode::Floating: return "GPIO_MODE INPUT\n";
      case IoInputMode::PullUp: return "GPIO_MODE PULLUP\n";
      case IoInputMode::PullDown: return "GPIO_MODE PULLDOWN\n";
    }
    return {};
  }

 private:
  template <typename ConfigurePin>
  void apply(IoInputMode mode, ConfigurePin configure) {
    for (std::size_t index = 0; index < board_.safePinCount; ++index) {
      configure(board_.safePins[index], mode);
    }
    mode_ = mode;
  }

  const BoardProfile &board_;
  IoInputMode mode_ = IoInputMode::Floating;
};
