#pragma once

#include <array>
#include <cstdint>
#include <optional>

enum class InputState {
  Down,
  Up,
};

struct InputEvent {
  std::uint32_t id;
  std::uint8_t gpio;
  InputState state;
};

enum class ResponseAction {
  Ignored,
  Cleared,
  Execute,
};

class GpioTriggerController {
 public:
  static constexpr std::uint32_t kDebounceMs = 30;
  static constexpr std::uint32_t kResponseTimeoutMs = 2000;
  static constexpr std::array<std::uint8_t, 17> kSupportedPins = {
      0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 12, 13, 14, 15, 16, 17, 18};

  explicit GpioTriggerController(std::uint32_t startMs = 0);

  static bool isSupportedPin(std::uint8_t gpio);
  std::optional<InputEvent> updatePin(std::uint8_t gpio, bool inputHigh,
                                      std::uint32_t nowMs);
  ResponseAction handleResponse(std::uint32_t eventId, bool execute);
  void expire(std::uint32_t nowMs);
  bool hasPendingEvent() const;

 private:
  struct PinState {
    bool rawHigh = true;
    bool stableHigh = true;
    std::uint32_t rawChangedMs = 0;
  };

  struct PendingEvent {
    std::uint32_t id;
    std::uint32_t startedMs;
  };

  static std::optional<std::size_t> pinIndex(std::uint8_t gpio);

  std::array<PinState, kSupportedPins.size()> pinStates_{};
  std::uint32_t nextEventId_ = 1;
  std::array<std::optional<PendingEvent>, kSupportedPins.size()>
      pendingEvents_{};
};
