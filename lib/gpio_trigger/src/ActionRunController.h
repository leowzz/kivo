#pragma once

#include <cstdint>
#include <optional>

#include "GpioTriggerController.h"

class ActionRunController {
 public:
  static constexpr std::uint32_t kResponseTimeoutMs = 2000;

  ResponseAction acceptStep(std::uint32_t runId, std::uint16_t step,
                            std::uint16_t total, std::uint32_t nowMs);
  ResponseAction cancel(std::uint32_t runId);
  bool keepAlive(std::uint32_t runId, std::uint32_t nowMs);
  void expire(std::uint32_t nowMs);
  bool hasActiveRun() const;

 private:
  struct ActiveRun {
    std::uint32_t id;
    std::uint16_t total;
    std::uint16_t nextStep;
    std::uint32_t updatedMs;
  };

  std::optional<ActiveRun> activeRun_;
};
