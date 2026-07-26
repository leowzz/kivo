#include <Arduino.h>
#include <esp_timer.h>

#include "LedController.h"

namespace {
LedController controller;

void applyOutputs(const LedOutputs &outputs) {
  digitalWrite(BoardPins::kLedA, outputs.ledA ? HIGH : LOW);
  digitalWrite(BoardPins::kLedB, outputs.ledB ? HIGH : LOW);
}
}  // namespace

void setup() {
  pinMode(BoardPins::kLedA, OUTPUT);
  pinMode(BoardPins::kLedB, OUTPUT);
  pinMode(BoardPins::kModeInput, INPUT_PULLUP);

  const auto nowUs = static_cast<std::uint64_t>(esp_timer_get_time());
  controller.reset(nowUs);
  applyOutputs(
      controller.update(nowUs, digitalRead(BoardPins::kModeInput) == HIGH));
}

void loop() {
  const auto nowUs = static_cast<std::uint64_t>(esp_timer_get_time());
  const bool inputHigh = digitalRead(BoardPins::kModeInput) == HIGH;
  applyOutputs(controller.update(nowUs, inputHigh));
}
