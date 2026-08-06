#include "DisplayStatus.h"

namespace {
constexpr std::size_t kDisplayColumns = 16;

std::string fitLine(std::string value) {
  if (value.size() > kDisplayColumns) value.resize(kDisplayColumns);
  value.append(kDisplayColumns - value.size(), ' ');
  return value;
}

std::string rightAlignedCount(std::string prefix, std::size_t count,
                              std::string suffix) {
  const auto countText = std::to_string(count);
  const auto occupied = prefix.size() + countText.size() + suffix.size();
  if (occupied < kDisplayColumns) {
    prefix.append(kDisplayColumns - occupied, ' ');
  }
  return fitLine(prefix + countText + suffix);
}

}  // namespace

void DisplayStatusModel::setReady(std::size_t keyCount) {
  mode_ = Mode::Ready;
  count_ = keyCount;
}

void DisplayStatusModel::setLearning(std::size_t pinCount) {
  mode_ = Mode::Learning;
  count_ = pinCount;
}

void DisplayStatusModel::setConfigError() { mode_ = Mode::ConfigError; }

void DisplayStatusModel::recordInput(const InputEvent &event) {
  lastInput_ = event.input;
  lastState_ = event.state;
}

void DisplayStatusModel::clearLastInput() { lastInput_.reset(); }

DisplayFrame DisplayStatusModel::frame() const {
  DisplayFrame result;
  result.lines[0] = standaloneDebug_
                        ? fitLine("KIVO GPIO DEBUG")
                        : usbConnected_ ? "KIVO      USB ON"
                                        : "KIVO     USB OFF";

  switch (mode_) {
    case Mode::Waiting:
      result.lines[1] = fitLine("WAITING CONFIG");
      break;
    case Mode::Ready:
      result.lines[1] = rightAlignedCount("READY", count_, " KEYS");
      break;
    case Mode::Learning:
      result.lines[1] = rightAlignedCount("LEARNING", count_, " PINS");
      break;
    case Mode::ConfigError:
      result.lines[1] = fitLine("CONFIG ERROR");
      break;
  }

  if (!lastInput_.has_value()) {
    result.lines[2].clear();
  } else {
    std::string input = std::to_string(lastInput_->pinA);
    if (lastInput_->kind == PhysicalInputKind::Contact) {
      input += "-" + std::to_string(lastInput_->pinB);
    }
    input += lastState_ == InputState::Down ? " D" : " U";
    result.lines[2] = input;
  }
  return result;
}
