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

std::optional<InputEvent> GpioTriggerController::updatePin(
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
  const InputState inputState = state.stableHigh ? InputState::Up
                                                 : InputState::Down;
  const InputEvent event{nextEventId_++, gpio, inputState};
  if (inputState == InputState::Down) {
    pendingEvents_[*index] = PendingEvent{event.id, nowMs};
  }
  return event;
}

ResponseAction GpioTriggerController::handleResponse(std::uint32_t eventId,
                                                     bool execute) {
  const auto pending =
      std::find_if(pendingEvents_.begin(), pendingEvents_.end(),
                   [eventId](const auto &entry) {
                     return entry.has_value() && entry->id == eventId;
                   });
  if (pending == pendingEvents_.end()) {
    return ResponseAction::Ignored;
  }

  pending->reset();
  return execute ? ResponseAction::Execute : ResponseAction::Cleared;
}

void GpioTriggerController::expire(std::uint32_t nowMs) {
  for (auto &entry : pendingEvents_) {
    if (entry.has_value() &&
        nowMs - entry->startedMs >= kResponseTimeoutMs) {
      entry.reset();
    }
  }
}

bool GpioTriggerController::hasPendingEvent() const {
  return std::any_of(pendingEvents_.begin(), pendingEvents_.end(),
                     [](const auto &entry) { return entry.has_value(); });
}
