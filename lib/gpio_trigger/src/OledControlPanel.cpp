#include "OledControlPanel.h"

#include <algorithm>
#include <array>
#include <string>

namespace {
constexpr std::size_t kDisplayColumns = 21;
constexpr std::uint8_t kMinimumBrightnessPercent = 5;
constexpr std::uint8_t kBrightnessStepPercent = 5;
constexpr std::size_t kBrightnessBarColumns = 16;
constexpr std::array<const char *, 5> kMenuEntries = {
    "LIVE VIEW", "SYSTEM STATUS", "INPUT TEST", "BRIGHTNESS", "DEVICE INFO"};
constexpr std::array<std::int8_t, 16> kEncoderTransitions = {
    0, -1, 1, 0, 1, 0, 0, -1, -1, 0, 0, 1, 0, 1, -1, 0};

std::string fitLine(std::string value) {
  if (value.size() > kDisplayColumns) value.resize(kDisplayColumns);
  return value;
}

std::string menuLine(bool selected, const char *label) {
  return fitLine(std::string(selected ? "> " : "  ") + label);
}

std::string brightnessBar(std::uint8_t percent) {
  const auto filled = std::max<std::size_t>(
      1, (static_cast<std::size_t>(percent) * kBrightnessBarColumns + 50) /
             100);
  return "[" + std::string(filled, '#') +
         std::string(kBrightnessBarColumns - filled, '.') + "]";
}
}  // namespace

bool OledControlPanel::DebouncedButton::update(bool pressed,
                                                std::uint32_t nowMs,
                                                std::uint16_t debounceMs) {
  if (pressed != rawPressed) {
    rawPressed = pressed;
    rawChangedMs = nowMs;
  }
  if (stablePressed == rawPressed || nowMs - rawChangedMs < debounceMs) {
    return false;
  }
  stablePressed = rawPressed;
  return stablePressed;
}

void OledControlPanel::reset() {
  view_ = View::Closed;
  selected_ = 0;
  confirm_ = {};
  encoderPress_ = {};
  back_ = {};
  encoderInitialized_ = false;
  encoderState_ = 0;
  encoderAccumulator_ = 0;
  encoderActivityInitialized_ = false;
  lastEncoderActivityMs_ = 0;
}

void OledControlPanel::setBrightnessPercent(std::uint8_t percent) {
  brightnessPercent_ = std::clamp(percent, kMinimumBrightnessPercent,
                                  static_cast<std::uint8_t>(100));
}

int OledControlPanel::encoderStep(const OledControlPanelSample &sample,
                                  std::uint32_t nowMs) {
  const auto current = static_cast<std::uint8_t>(
      (sample.encoderAHigh ? 2U : 0U) | (sample.encoderBHigh ? 1U : 0U));
  if (!encoderInitialized_) {
    encoderInitialized_ = true;
    encoderState_ = current;
    return 0;
  }
  const auto transition = static_cast<std::uint8_t>((encoderState_ << 2U) |
                                                     current);
  if (current != encoderState_) {
    encoderActivityInitialized_ = true;
    lastEncoderActivityMs_ = nowMs;
  }
  encoderState_ = current;
  encoderAccumulator_ += kEncoderTransitions[transition];
  if (encoderAccumulator_ >= 4) {
    encoderAccumulator_ = 0;
    return 1;
  }
  if (encoderAccumulator_ <= -4) {
    encoderAccumulator_ = 0;
    return -1;
  }
  return 0;
}

OledControlPanelUpdate OledControlPanel::select() {
  if (view_ == View::Closed) {
    view_ = View::Menu;
    return OledControlPanelUpdate::Render;
  }
  if (view_ != View::Menu) {
    view_ = View::Menu;
    return OledControlPanelUpdate::Render;
  }
  switch (selected_) {
    case 0:
      view_ = View::Closed;
      return OledControlPanelUpdate::Dismiss;
    case 1:
      view_ = View::Status;
      break;
    case 2:
      view_ = View::InputTest;
      break;
    case 3:
      view_ = View::Brightness;
      break;
    default:
      view_ = View::DeviceInfo;
      break;
  }
  return OledControlPanelUpdate::Render;
}

OledControlPanelUpdate OledControlPanel::update(
    const OledControlPanelSample &sample, std::uint32_t nowMs,
    std::uint16_t debounceMs) {
  const int step = encoderStep(sample, nowMs);
  const bool confirmPressed =
      confirm_.update(sample.confirmPressed, nowMs, debounceMs);
  const bool encoderPressed =
      encoderPress_.update(sample.encoderPressed, nowMs, debounceMs);
  const bool backPressed = back_.update(sample.backPressed, nowMs, debounceMs);

  // EC11 contacts can couple into the push/back lines while rotating. Keep
  // sampling those buttons so their debouncers recover, but do not dispatch
  // a button action until the encoder has been quiet for two debounce windows.
  const auto encoderSettled =
      !encoderActivityInitialized_ ||
      nowMs - lastEncoderActivityMs_ >=
          static_cast<std::uint32_t>(debounceMs) * 2U;
  const auto encoderPressStartedDuringMotion =
      encoderActivityInitialized_ &&
      encoderPress_.changedAtOrBefore(lastEncoderActivityMs_);
  const auto backStartedDuringMotion =
      encoderActivityInitialized_ &&
      back_.changedAtOrBefore(lastEncoderActivityMs_);
  const bool selectPressed =
      confirmPressed || (encoderPressed && !encoderPressStartedDuringMotion);

  if (step != 0) {
    if (view_ == View::Brightness) {
      const auto next = std::clamp(
          static_cast<int>(brightnessPercent_) +
              step * static_cast<int>(kBrightnessStepPercent),
          static_cast<int>(kMinimumBrightnessPercent), 100);
      if (next == brightnessPercent_) return OledControlPanelUpdate::None;
      brightnessPercent_ = static_cast<std::uint8_t>(next);
      return OledControlPanelUpdate::BrightnessChanged;
    }
    if (view_ == View::Closed) {
      // Rotation is also a useful menu entry gesture when the panel is idle.
      view_ = View::Menu;
    } else if (view_ != View::Menu) {
      return OledControlPanelUpdate::None;
    }

    const auto entryCount = static_cast<int>(kMenuEntries.size());
    selected_ = static_cast<std::uint8_t>(
        (static_cast<int>(selected_) + step + entryCount) % entryCount);
    return OledControlPanelUpdate::Render;
  }
  if (!encoderSettled) return OledControlPanelUpdate::None;
  if (selectPressed) return select();
  if (backPressed && !backStartedDuringMotion) {
    if (view_ == View::Closed) return OledControlPanelUpdate::None;
    if (view_ == View::Menu) {
      view_ = View::Closed;
      return OledControlPanelUpdate::Dismiss;
    }
    view_ = View::Menu;
    return OledControlPanelUpdate::Render;
  }
  return OledControlPanelUpdate::None;
}

DisplayFrame OledControlPanel::frame(const DisplayFrame &status) const {
  DisplayFrame result{};
  switch (view_) {
    case View::Closed:
      return status;
    case View::Menu: {
      result.lines[0] = "KIVO MENU";
      const std::size_t start = selected_ < 3 ? 0 : selected_ - 2;
      for (std::size_t row = 0; row < 3; ++row) {
        const std::size_t entry = start + row;
        result.lines[row + 1] =
            menuLine(entry == selected_, kMenuEntries[entry]);
      }
      break;
    }
    case View::Status:
      result.lines[0] = "SYSTEM STATUS";
      result.lines[1] = fitLine(status.lines[0]);
      result.lines[2] = fitLine(status.lines[1]);
      result.lines[3] = status.lines[2].empty() ? "NO KEY EVENT"
                                               : fitLine(status.lines[2]);
      break;
    case View::InputTest:
      result.lines[0] = "INPUT TEST";
      result.lines[1] = "PRESS ANY KEY";
      result.lines[2] = status.lines[2].empty() ? "NO KEY EVENT"
                                               : fitLine(status.lines[2]);
      result.lines[3] = "BACK: MENU";
      break;
    case View::Brightness:
      result.lines[0] = "DISPLAY BRIGHTNESS";
      result.lines[1] = "LEVEL: " + std::to_string(brightnessPercent_) + "%";
      result.lines[2] = brightnessBar(brightnessPercent_);
      result.lines[3] = "OK/BACK: MENU";
      break;
    case View::DeviceInfo:
      result.lines[0] = "DEVICE INFO";
      result.lines[1] = "SH1106 128X64";
      result.lines[2] = "I2C 0X3C / 1.3IN";
      result.lines[3] = "EC11 + OK/BACK";
      break;
  }
  return result;
}
