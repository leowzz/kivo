#pragma once

#include <cstdint>

#include "DisplayStatus.h"

struct OledControlPanelSample {
  bool confirmPressed = false;
  bool encoderPressed = false;
  bool encoderAHigh = true;
  bool encoderBHigh = true;
  bool backPressed = false;
};

enum class OledControlPanelUpdate { None, Render, Dismiss, BrightnessChanged };

class OledControlPanel {
 public:
  void reset();
  void setBrightnessPercent(std::uint8_t percent);
  OledControlPanelUpdate update(const OledControlPanelSample &sample,
                                std::uint32_t nowMs,
                                std::uint16_t debounceMs);
  bool active() const { return view_ != View::Closed; }
  std::uint8_t brightnessPercent() const { return brightnessPercent_; }
  DisplayFrame frame(const DisplayFrame &status) const;

 private:
  struct DebouncedButton {
    bool rawPressed = false;
    bool stablePressed = false;
    std::uint32_t rawChangedMs = 0;

    bool update(bool pressed, std::uint32_t nowMs,
                std::uint16_t debounceMs);
    bool changedAtOrBefore(std::uint32_t timestamp) const {
      return rawChangedMs <= timestamp;
    }
  };

  enum class View { Closed, Menu, Status, InputTest, Brightness, DeviceInfo };

  int encoderStep(const OledControlPanelSample &sample,
                  std::uint32_t nowMs);
  OledControlPanelUpdate select();

  View view_ = View::Closed;
  std::uint8_t selected_ = 0;
  DebouncedButton confirm_;
  DebouncedButton encoderPress_;
  DebouncedButton back_;
  bool encoderInitialized_ = false;
  std::uint8_t encoderState_ = 0;
  std::int8_t encoderAccumulator_ = 0;
  bool encoderActivityInitialized_ = false;
  std::uint32_t lastEncoderActivityMs_ = 0;
  std::uint8_t brightnessPercent_ = 100;
};
