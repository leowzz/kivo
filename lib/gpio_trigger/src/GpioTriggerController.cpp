#include "GpioTriggerController.h"

#include <algorithm>
#include <vector>

GpioTriggerController::GpioTriggerController(const BoardProfile &profile,
                                             std::uint32_t)
    : profile_(profile) {}

bool GpioTriggerController::isSupportedPin(std::uint8_t gpio) const {
  return profile_.supports(gpio);
}

void GpioTriggerController::configure(const RuntimeTopology &topology,
                                      std::uint32_t nowMs) {
  topology_ = topology;
  inputs_.clear();
  for (const auto &source : topology.directs) {
    for (const auto pin : source.pins) {
      inputs_.push_back({PhysicalInput::direct(pin), false, false, false,
                         nowMs});
    }
  }
  for (const auto &source : topology.matrices) {
    for (const auto row : source.rows) {
      for (const auto column : source.columns) {
        inputs_.push_back({PhysicalInput::contact(source.index, row, column),
                           false, false, false, nowMs});
      }
    }
  }
  pendingEvents_.assign(inputs_.size(), std::nullopt);
}

std::optional<std::size_t> GpioTriggerController::inputIndex(
    const PhysicalInput &input) const {
  const auto found = std::find_if(
      inputs_.begin(), inputs_.end(), [&input](const auto &entry) {
        return entry.input == input;
      });
  if (found == inputs_.end()) return std::nullopt;
  return static_cast<std::size_t>(found - inputs_.begin());
}

std::optional<InputEvent> GpioTriggerController::updatePin(
    std::uint8_t gpio, bool inputHigh, std::uint32_t nowMs) {
  const auto index = inputIndex(PhysicalInput::direct(gpio));
  return index.has_value() ? updateInput(*index, !inputHigh, nowMs)
                           : std::nullopt;
}

std::optional<InputEvent> GpioTriggerController::updateContact(
    std::uint8_t sourceIndex, std::uint8_t pinA, std::uint8_t pinB, bool closed,
    std::uint32_t nowMs) {
  const auto index =
      inputIndex(PhysicalInput::contact(sourceIndex, pinA, pinB));
  return index.has_value() ? updateInput(*index, closed, nowMs) : std::nullopt;
}

bool GpioTriggerController::beginLearning(
    std::uint32_t revision, const std::vector<std::uint8_t> &pins,
    std::uint32_t nowMs) {
  if (learningRevision_.has_value() || pins.empty()) return false;
  for (const auto pin : pins) {
    if (!isSupportedPin(pin) ||
        std::count(pins.begin(), pins.end(), pin) != 1 ||
        (topology_.oled.has_value() &&
         (pin == topology_.oled->sda || pin == topology_.oled->scl))) {
      return false;
    }
  }
  learningRevision_ = revision;
  learningPins_ = pins;
  inputs_.clear();
  for (const auto pin : pins) {
    inputs_.push_back(
        {PhysicalInput::direct(pin), false, false, false, nowMs});
  }
  for (std::size_t left = 0; left < pins.size(); ++left) {
    for (std::size_t right = left + 1; right < pins.size(); ++right) {
      inputs_.push_back({PhysicalInput::contact(0, pins[left], pins[right]),
                         false, false, false, nowMs});
    }
  }
  pendingEvents_.assign(inputs_.size(), std::nullopt);
  return true;
}

bool GpioTriggerController::endLearning(std::uint32_t revision,
                                        std::uint32_t nowMs) {
  if (learningRevision_ != revision) return false;
  learningRevision_.reset();
  learningPins_.clear();
  configure(topology_, nowMs);
  return true;
}

std::optional<InputEvent> GpioTriggerController::updateLearningPin(
    std::uint8_t gpio, bool inputHigh, std::uint32_t nowMs) {
  if (!isLearning()) return std::nullopt;
  return updatePin(gpio, inputHigh, nowMs);
}

std::optional<InputEvent> GpioTriggerController::updateLearningContact(
    std::uint8_t pinA, std::uint8_t pinB, bool closed, std::uint32_t nowMs) {
  if (!isLearning()) return std::nullopt;
  return updateContact(0, pinA, pinB, closed, nowMs);
}

std::optional<InputEvent> GpioTriggerController::updateInput(
    std::size_t index, bool active, std::uint32_t nowMs) {
  expire(nowMs);
  auto &state = inputs_[index];
  if (active != state.rawActive) {
    state.rawActive = active;
    state.rawChangedMs = nowMs;
  }

  if (state.rawActive == state.stableActive ||
      nowMs - state.rawChangedMs < topology_.debounceMs) {
    return std::nullopt;
  }

  state.stableActive = state.rawActive;
  if (state.stableActive && state.input.kind == PhysicalInputKind::Contact &&
      createsContactCycle(index)) {
    state.reportedActive = false;
    return std::nullopt;
  }
  if (!state.stableActive && !state.reportedActive) return std::nullopt;

  state.reportedActive = state.stableActive;
  const auto inputState = state.stableActive ? InputState::Down : InputState::Up;
  const InputEvent event{nextEventId_++, state.input, inputState};
  if (inputState == InputState::Down && !isLearning()) {
    pendingEvents_[index] = PendingEvent{event.id, 1, 0, nowMs};
  }
  return event;
}

bool GpioTriggerController::createsContactCycle(std::size_t candidate) const {
  const auto &input = inputs_[candidate].input;
  std::vector<std::uint8_t> pending{input.pinA};
  std::vector<std::uint8_t> visited;
  while (!pending.empty()) {
    const auto pin = pending.back();
    pending.pop_back();
    if (pin == input.pinB) return true;
    if (std::find(visited.begin(), visited.end(), pin) != visited.end()) continue;
    visited.push_back(pin);
    for (std::size_t index = 0; index < inputs_.size(); ++index) {
      if (index == candidate || !inputs_[index].stableActive ||
          !inputs_[index].reportedActive) {
        continue;
      }
      const auto &edge = inputs_[index].input;
      if (edge.kind != PhysicalInputKind::Contact ||
          edge.sourceIndex != input.sourceIndex) {
        continue;
      }
      if (edge.pinA == pin) pending.push_back(edge.pinB);
      if (edge.pinB == pin) pending.push_back(edge.pinA);
    }
  }
  return false;
}

ResponseAction GpioTriggerController::acceptStep(
    std::uint32_t eventId, std::uint16_t step, std::uint16_t total,
    bool execute, std::uint32_t nowMs) {
  expire(nowMs);
  const auto pending =
      std::find_if(pendingEvents_.begin(), pendingEvents_.end(),
                   [eventId](const auto &entry) {
                     return entry.has_value() && entry->id == eventId;
                   });
  if (pending == pendingEvents_.end()) return ResponseAction::Ignored;
  if (!execute) {
    pending->reset();
    return ResponseAction::Cleared;
  }
  if (step == 0 || total == 0 || step > total || step != (*pending)->nextStep ||
      ((*pending)->total != 0 && (*pending)->total != total)) {
    return ResponseAction::Ignored;
  }
  (*pending)->total = total;
  (*pending)->updatedMs = nowMs;
  if (step == total) {
    pending->reset();
  } else {
    ++(*pending)->nextStep;
  }
  return ResponseAction::Execute;
}

void GpioTriggerController::expire(std::uint32_t nowMs) {
  for (auto &entry : pendingEvents_) {
    if (entry.has_value() &&
        nowMs - entry->updatedMs >= kResponseTimeoutMs) {
      entry.reset();
    }
  }
}

bool GpioTriggerController::hasPendingEvent() const {
  return std::any_of(pendingEvents_.begin(), pendingEvents_.end(),
                     [](const auto &entry) { return entry.has_value(); });
}
