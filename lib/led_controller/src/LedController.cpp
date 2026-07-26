#include "LedController.h"

LedController::LedController(std::uint64_t startUs) { reset(startUs); }

void LedController::reset(std::uint64_t startUs) {
  blinkLedA_ = true;
  blinkOn_ = true;
  nextBlinkToggleUs_ = startUs + kHalfPeriodUs;
}

LedOutputs LedController::update(std::uint64_t nowUs, bool) {
  if (nowUs >= nextBlinkToggleUs_) {
    const std::uint64_t intervals =
        ((nowUs - nextBlinkToggleUs_) / kHalfPeriodUs) + 1;
    if ((intervals & 1U) != 0U) {
      blinkOn_ = !blinkOn_;
    }
    nextBlinkToggleUs_ += intervals * kHalfPeriodUs;
  }

  return blinkLedA_ ? LedOutputs{blinkOn_, true}
                    : LedOutputs{true, blinkOn_};
}
