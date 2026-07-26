#include "LedController.h"

LedController::LedController(std::uint64_t startUs) { reset(startUs); }

void LedController::reset(std::uint64_t startUs) {
  blinkLedA_ = true;
  blinkOn_ = true;
  nextBlinkToggleUs_ = startUs + kHalfPeriodUs;
  rawInputHigh_ = true;
  stableInputHigh_ = true;
  inputArmed_ = true;
  rawInputChangedUs_ = startUs;
}

void LedController::updateInput(std::uint64_t nowUs, bool inputHigh) {
  if (inputHigh != rawInputHigh_) {
    rawInputHigh_ = inputHigh;
    rawInputChangedUs_ = nowUs;
  }

  if (rawInputHigh_ == stableInputHigh_ ||
      nowUs - rawInputChangedUs_ < kDebounceUs) {
    return;
  }

  stableInputHigh_ = rawInputHigh_;
  if (stableInputHigh_) {
    inputArmed_ = true;
    return;
  }

  if (inputArmed_) {
    blinkLedA_ = !blinkLedA_;
    blinkOn_ = true;
    nextBlinkToggleUs_ = nowUs + kHalfPeriodUs;
    inputArmed_ = false;
  }
}

LedOutputs LedController::update(std::uint64_t nowUs, bool inputHigh) {
  updateInput(nowUs, inputHigh);

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
