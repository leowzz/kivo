#include "OledControlPanel.h"

#include <algorithm>
#include <array>
#include <string>

namespace {
constexpr std::size_t kDisplayColumns = 21;
constexpr std::array<const char *, 4> kMenuEntries = {
    "LIVE VIEW", "SYSTEM STATUS", "INPUT TEST", "DEVICE INFO"};
constexpr std::array<std::int8_t, 16> kEncoderTransitions = {
    0, -1, 1, 0, 1, 0, 0, -1, -1, 0, 0, 1, 0, 1, -1, 0};

std::string fitLine(std::string value) {
  if (value.size() > kDisplayColumns) value.resize(kDisplayColumns);
  return value;
}

std::string menuLine(bool selected, const char *label) {
  return fitLine(std::string(selected ? "> " : "  ") + label);
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
}

int OledControlPanel::encoderStep(const OledControlPanelSample &sample) {
  const auto current = static_cast<std::uint8_t>(
      (sample.encoderAHigh ? 2U : 0U) | (sample.encoderBHigh ? 1U : 0U));
  if (!encoderInitialized_) {
    encoderInitialized_ = true;
    encoderState_ = current;
    return 0;
  }
  const auto transition = static_cast<std::uint8_t>((encoderState_ << 2U) |
                                                     current);
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
    default:
      view_ = View::DeviceInfo;
      break;
  }
  return OledControlPanelUpdate::Render;
}

OledControlPanelUpdate OledControlPanel::update(
    const OledControlPanelSample &sample, std::uint32_t nowMs,
    std::uint16_t debounceMs) {
  const bool selectPressed = confirm_.update(sample.confirmPressed, nowMs,
                                              debounceMs) |
                             encoderPress_.update(sample.encoderPressed, nowMs,
                                                  debounceMs);
  const bool backPressed = back_.update(sample.backPressed, nowMs, debounceMs);
  const int step = encoderStep(sample);

  if (selectPressed) return select();
  if (backPressed) {
    if (view_ == View::Closed) return OledControlPanelUpdate::None;
    if (view_ == View::Menu) {
      view_ = View::Closed;
      return OledControlPanelUpdate::Dismiss;
    }
    view_ = View::Menu;
    return OledControlPanelUpdate::Render;
  }
  if (view_ != View::Menu || step == 0) return OledControlPanelUpdate::None;

  const auto entryCount = static_cast<int>(kMenuEntries.size());
  selected_ = static_cast<std::uint8_t>(
      (static_cast<int>(selected_) + step + entryCount) % entryCount);
  return OledControlPanelUpdate::Render;
}

DisplayFrame OledControlPanel::frame(const DisplayFrame &status) const {
  DisplayFrame result{};
  switch (view_) {
    case View::Closed:
      return status;
    case View::Menu: {
      result.lines[0] = "KIVO MENU";
      const std::size_t start = selected_ == 3 ? 1 : 0;
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
    case View::DeviceInfo:
      result.lines[0] = "DEVICE INFO";
      result.lines[1] = "SH1106 128X64";
      result.lines[2] = "I2C 0X3C / 1.3IN";
      result.lines[3] = "EC11 + OK/BACK";
      break;
  }
  return result;
}
