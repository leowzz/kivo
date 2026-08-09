#include "ActionRunController.h"

ResponseAction ActionRunController::acceptStep(std::uint32_t runId,
                                               std::uint16_t step,
                                               std::uint16_t total,
                                               std::uint32_t nowMs) {
  expire(nowMs);
  if (runId == 0 || step == 0 || total == 0 || step > total) {
    return ResponseAction::Ignored;
  }

  if (!activeRun_.has_value()) {
    if (step != 1) return ResponseAction::Ignored;
    activeRun_ = ActiveRun{runId, total, 2, nowMs};
  } else if (activeRun_->id != runId || activeRun_->total != total ||
             activeRun_->nextStep != step) {
    return ResponseAction::Ignored;
  } else {
    activeRun_->updatedMs = nowMs;
    ++activeRun_->nextStep;
  }

  if (step == total) activeRun_.reset();
  return ResponseAction::Execute;
}

ResponseAction ActionRunController::cancel(std::uint32_t runId) {
  if (!activeRun_.has_value() || activeRun_->id != runId) {
    return ResponseAction::Ignored;
  }
  activeRun_.reset();
  return ResponseAction::Cleared;
}

bool ActionRunController::keepAlive(std::uint32_t runId, std::uint32_t nowMs) {
  expire(nowMs);
  if (!activeRun_.has_value() || activeRun_->id != runId) return false;
  activeRun_->updatedMs = nowMs;
  return true;
}

void ActionRunController::expire(std::uint32_t nowMs) {
  if (activeRun_.has_value() &&
      nowMs - activeRun_->updatedMs >= kResponseTimeoutMs) {
    activeRun_.reset();
  }
}

void ActionRunController::reset() { activeRun_.reset(); }

bool ActionRunController::hasActiveRun() const { return activeRun_.has_value(); }
