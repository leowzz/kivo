#include "GpioTriggerController.h"

#include <algorithm>

GpioTriggerController::GpioTriggerController(std::uint32_t startMs) {
  for (auto &state : pinStates_) {
    state.rawChangedMs = startMs;
  }
}

bool GpioTriggerController::isSupportedPin(std::uint8_t gpio) {
  return pinIndex(gpio).has_value();
}

std::optional<std::size_t> GpioTriggerController::pinIndex(std::uint8_t gpio) {
  const auto found =
      std::find(kSupportedPins.begin(), kSupportedPins.end(), gpio);
  if (found == kSupportedPins.end()) {
    return std::nullopt;
  }
  return static_cast<std::size_t>(found - kSupportedPins.begin());
}

std::optional<PressEvent> GpioTriggerController::updatePin(
    std::uint8_t gpio, bool inputHigh, std::uint32_t nowMs) {
  expire(nowMs);

  const auto index = pinIndex(gpio);
  if (!index.has_value()) {
    return std::nullopt;
  }

  auto &state = pinStates_[*index];
  if (inputHigh != state.rawHigh) {
    state.rawHigh = inputHigh;
    state.rawChangedMs = nowMs;
  }

  if (state.rawHigh == state.stableHigh ||
      nowMs - state.rawChangedMs < kDebounceMs) {
    return std::nullopt;
  }

  state.stableHigh = state.rawHigh;
  if (state.stableHigh || pendingEvent_.has_value()) {
    return std::nullopt;
  }

  pendingEvent_ = PressEvent{nextEventId_++, gpio};
  pendingStartedMs_ = nowMs;
  return pendingEvent_;
}

ResponseAction GpioTriggerController::handleResponse(std::uint32_t eventId,
                                                     bool execute) {
  if (!pendingEvent_.has_value() || pendingEvent_->id != eventId) {
    return ResponseAction::Ignored;
  }

  pendingEvent_.reset();
  return execute ? ResponseAction::Execute : ResponseAction::Cleared;
}

void GpioTriggerController::expire(std::uint32_t nowMs) {
  if (pendingEvent_.has_value() &&
      nowMs - pendingStartedMs_ >= kResponseTimeoutMs) {
    pendingEvent_.reset();
  }
}

bool GpioTriggerController::hasPendingEvent() const {
  return pendingEvent_.has_value();
}
