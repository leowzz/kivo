#pragma once

#include <cstdint>

#include "DisplayStatus.h"
#include "DisplayConfig.h"

struct ControlPanelSample {
  bool confirmPressed = false;
  bool encoderPressed = false;
  bool encoderAHigh = true;
  bool encoderBHigh = true;
  bool backPressed = false;
};

enum class ControlPanelUpdate { None, Render, Dismiss, BrightnessChanged, AnimationChanged };

class ControlPanel {
 public:
  void reset();
  void setBrightnessPercent(std::uint8_t percent);
  void setAnimation(std::uint8_t animation);
  std::uint8_t animation() const { return static_cast<std::uint8_t>(animation_); }
  void setCpuPercent(std::uint8_t percent, std::uint32_t nowMs);
  void clearCpuPercent() { cpuPercent_.reset(); }
  bool tickAnimation(std::uint32_t nowMs);
  ControlPanelUpdate update(const ControlPanelSample &sample,
                                std::uint32_t nowMs,
                                std::uint16_t debounceMs);
  bool active() const { return view_ != View::Closed; }
  std::uint8_t brightnessPercent() const { return brightnessPercent_; }
  DisplayFrame frame(const DisplayFrame &status, const DisplayConfig &display) const;

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

  enum class View {
    Closed,
    Menu,
    Status,
    InputTest,
    Brightness,
    DeviceInfo,
    RunCat
  };

  int encoderStep(const ControlPanelSample &sample,
                  std::uint32_t nowMs);
  ControlPanelUpdate select();

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
  RunCatAnimation animation_ = RunCatAnimation::Cat;
  std::uint8_t animationPhase_ = 0;
  std::optional<std::uint8_t> cpuPercent_;
  std::uint32_t cpuUpdatedMs_ = 0;
  std::uint32_t animationUpdatedMs_ = 0;
};
