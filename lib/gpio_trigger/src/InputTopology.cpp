#include "InputTopology.h"

#include <algorithm>

PhysicalInput PhysicalInput::direct(std::uint8_t gpio) {
  return {PhysicalInputKind::Direct, 0, gpio, gpio};
}

PhysicalInput PhysicalInput::contact(std::uint8_t sourceIndex,
                                     std::uint8_t pinA, std::uint8_t pinB) {
  if (pinB < pinA) std::swap(pinA, pinB);
  return {PhysicalInputKind::Contact, sourceIndex, pinA, pinB};
}

bool PhysicalInput::operator==(const PhysicalInput &other) const {
  return kind == other.kind && sourceIndex == other.sourceIndex &&
         pinA == other.pinA && pinB == other.pinB;
}

std::size_t RuntimeTopology::keyCount() const {
  std::size_t count = 0;
  for (const auto &source : directs) count += source.pins.size();
  for (const auto &source : matrices) {
    count += source.rows.size() * source.columns.size();
  }
  return count;
}

bool TopologyBuilder::begin(std::uint32_t revision,
                            std::uint16_t debounceMs) {
  if (debounceMs == 0 || debounceMs > 1000) return false;
  pending_ = RuntimeTopology{revision, debounceMs, {}, {}, std::nullopt,
                             std::nullopt};
  sourceIndices_.clear();
  ownedPins_.clear();
  return true;
}

bool TopologyBuilder::pinsAvailable(
    const std::vector<std::uint8_t> &pins) const {
  if (pins.empty()) return false;
  for (const auto pin : pins) {
    if (!profile_.supports(pin) ||
        std::find(ownedPins_.begin(), ownedPins_.end(), pin) !=
            ownedPins_.end() ||
        std::count(pins.begin(), pins.end(), pin) != 1) {
      return false;
    }
  }
  return true;
}

bool TopologyBuilder::addPins(std::uint8_t sourceIndex,
                              const std::vector<std::uint8_t> &pins) {
  if (!pending_.has_value() ||
      std::find(sourceIndices_.begin(), sourceIndices_.end(), sourceIndex) !=
          sourceIndices_.end() ||
      !pinsAvailable(pins)) {
    return false;
  }
  sourceIndices_.push_back(sourceIndex);
  ownedPins_.insert(ownedPins_.end(), pins.begin(), pins.end());
  return true;
}

bool TopologyBuilder::addOled(std::uint32_t revision, std::uint8_t sda,
                              std::uint8_t scl) {
  return addOled(revision, sda, scl, OledDriver::Ssd1306);
}

bool TopologyBuilder::addSh1106(std::uint32_t revision, std::uint8_t sda,
                                std::uint8_t scl) {
  return addOled(revision, sda, scl, OledDriver::Sh1106);
}

bool TopologyBuilder::addOled(std::uint32_t revision, std::uint8_t sda,
                              std::uint8_t scl, OledDriver driver) {
  const std::vector<std::uint8_t> pins{sda, scl};
  if (!pending_.has_value() || pending_->revision != revision ||
      !profile_.supportsOled || pending_->oled.has_value() ||
      !pinsAvailable(pins)) {
    return false;
  }
  ownedPins_.insert(ownedPins_.end(), pins.begin(), pins.end());
  pending_->oled = OledConfig{sda, scl, driver};
  return true;
}

bool TopologyBuilder::addOledControlPanel(
    std::uint32_t revision, std::uint8_t confirm,
    std::uint8_t encoderPress, std::uint8_t encoderA,
    std::uint8_t encoderB, std::uint8_t back) {
  const std::vector<std::uint8_t> pins{confirm, encoderPress, encoderA,
                                       encoderB, back};
  if (!pending_.has_value() || pending_->revision != revision ||
      !pending_->oled.has_value() || pending_->oledControlPanel.has_value() ||
      !pinsAvailable(pins)) {
    return false;
  }
  ownedPins_.insert(ownedPins_.end(), pins.begin(), pins.end());
  pending_->oledControlPanel =
      OledControlPanelConfig{confirm, encoderPress, encoderA, encoderB, back};
  return true;
}

bool TopologyBuilder::addDirect(std::uint32_t revision,
                                std::uint8_t sourceIndex,
                                std::vector<std::uint8_t> pins) {
  if (!pending_.has_value() || pending_->revision != revision ||
      !addPins(sourceIndex, pins)) {
    return false;
  }
  pending_->directs.push_back({sourceIndex, std::move(pins)});
  return true;
}

bool TopologyBuilder::addMatrix(std::uint32_t revision,
                                std::uint8_t sourceIndex,
                                std::vector<std::uint8_t> rows,
                                std::vector<std::uint8_t> columns) {
  if (!pending_.has_value() || pending_->revision != revision || rows.empty() ||
      columns.empty()) {
    return false;
  }
  std::vector<std::uint8_t> pins = rows;
  pins.insert(pins.end(), columns.begin(), columns.end());
  if (!addPins(sourceIndex, pins)) return false;
  pending_->matrices.push_back(
      {sourceIndex, std::move(rows), std::move(columns)});
  return true;
}

std::optional<RuntimeTopology> TopologyBuilder::commit(
    std::uint32_t revision) {
  if (!pending_.has_value() || pending_->revision != revision) {
    return std::nullopt;
  }
  auto sorted = sourceIndices_;
  std::sort(sorted.begin(), sorted.end());
  for (std::size_t index = 0; index < sorted.size(); ++index) {
    if (sorted[index] != index) return std::nullopt;
  }
  auto result = std::move(pending_);
  pending_.reset();
  sourceIndices_.clear();
  ownedPins_.clear();
  return result;
}

void TopologyBuilder::cancel() {
  pending_.reset();
  sourceIndices_.clear();
  ownedPins_.clear();
}
