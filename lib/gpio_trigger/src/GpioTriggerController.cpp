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

std::optional<InputEvent> GpioTriggerController::updateInput(
    std::size_t index, bool active, std::uint32_t nowMs) {
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
  return InputEvent{nextEventId_++, state.input, inputState};
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
