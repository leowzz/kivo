#pragma once

#include <cstdint>

struct LedOutputs {
  bool ledA;
  bool ledB;
};

class LedController {
 public:
  static constexpr std::uint64_t kHalfPeriodUs = 166667;
  static constexpr std::uint64_t kDebounceUs = 30000;

  explicit LedController(std::uint64_t startUs = 0);

  void reset(std::uint64_t startUs);
  LedOutputs update(std::uint64_t nowUs, bool inputHigh);

 private:
  bool blinkLedA_ = true;
  bool blinkOn_ = true;
  std::uint64_t nextBlinkToggleUs_ = kHalfPeriodUs;
};
