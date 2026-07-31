#pragma once

#include <cstdint>
#include <optional>
#include <vector>

#include "InputTopology.h"

enum class InputState {
  Down,
  Up,
};

struct InputEvent {
  std::uint32_t id;
  PhysicalInput input;
  InputState state;
  std::uint8_t gpio;

  InputEvent(std::uint32_t id, std::uint8_t gpio, InputState state)
      : id(id), input(PhysicalInput::direct(gpio)), state(state), gpio(gpio) {}
  InputEvent(std::uint32_t id, PhysicalInput input, InputState state)
      : id(id), input(input), state(state), gpio(input.pinA) {}
};

enum class ResponseAction {
  Ignored,
  Cleared,
  Execute,
};

class GpioTriggerController {
 public:
  static constexpr std::uint32_t kResponseTimeoutMs = 2000;

  explicit GpioTriggerController(const BoardProfile &profile,
                                 std::uint32_t startMs = 0);

  bool isSupportedPin(std::uint8_t gpio) const;
  void configure(const RuntimeTopology &topology, std::uint32_t nowMs);
  std::optional<InputEvent> updatePin(std::uint8_t gpio, bool inputHigh,
                                      std::uint32_t nowMs);
  std::optional<InputEvent> updateContact(std::uint8_t sourceIndex,
                                          std::uint8_t pinA,
                                          std::uint8_t pinB, bool closed,
                                          std::uint32_t nowMs);
  bool beginLearning(std::uint32_t revision,
                     const std::vector<std::uint8_t> &pins,
                     std::uint32_t nowMs);
  bool endLearning(std::uint32_t revision, std::uint32_t nowMs);
  std::optional<InputEvent> updateLearningPin(std::uint8_t gpio,
                                              bool inputHigh,
                                              std::uint32_t nowMs);
  std::optional<InputEvent> updateLearningContact(std::uint8_t pinA,
                                                  std::uint8_t pinB,
                                                  bool closed,
                                                  std::uint32_t nowMs);
  bool isLearning() const { return learningRevision_.has_value(); }
  const std::vector<std::uint8_t> &learningPins() const {
    return learningPins_;
  }
  ResponseAction acceptStep(std::uint32_t eventId, std::uint16_t step,
                            std::uint16_t total, bool execute,
                            std::uint32_t nowMs);
  void expire(std::uint32_t nowMs);
  bool hasPendingEvent() const;
  const RuntimeTopology &topology() const { return topology_; }

 private:
  struct InputSlot {
    PhysicalInput input;
    bool rawActive = false;
    bool stableActive = false;
    bool reportedActive = false;
    std::uint32_t rawChangedMs = 0;
  };

  struct PendingEvent {
    std::uint32_t id;
    std::uint16_t nextStep = 1;
    std::uint16_t total = 0;
    std::uint32_t updatedMs;
  };

  std::optional<std::size_t> inputIndex(const PhysicalInput &input) const;
  std::optional<InputEvent> updateInput(std::size_t index, bool active,
                                        std::uint32_t nowMs);
  bool createsContactCycle(std::size_t candidate) const;

  RuntimeTopology topology_;
  const BoardProfile &profile_;
  std::vector<InputSlot> inputs_;
  std::optional<std::uint32_t> learningRevision_;
  std::vector<std::uint8_t> learningPins_;
  std::uint32_t nextEventId_ = 1;
  std::vector<std::optional<PendingEvent>> pendingEvents_;
};
