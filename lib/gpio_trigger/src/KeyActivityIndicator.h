#pragma once

#include <cstddef>
#include <limits>

#include "GpioTriggerController.h"

enum class KeyIndicatorAction {
  None,
  ShowRandomColor,
  Off,
};

class KeyActivityIndicator {
 public:
  KeyIndicatorAction handle(InputState state) {
    if (state == InputState::Down) {
      if (activeCount_ < std::numeric_limits<std::size_t>::max()) {
        ++activeCount_;
      }
      return KeyIndicatorAction::ShowRandomColor;
    }
    if (activeCount_ == 0) return KeyIndicatorAction::None;
    --activeCount_;
    return activeCount_ == 0 ? KeyIndicatorAction::Off
                             : KeyIndicatorAction::None;
  }

  void reset() { activeCount_ = 0; }

 private:
  std::size_t activeCount_ = 0;
};
