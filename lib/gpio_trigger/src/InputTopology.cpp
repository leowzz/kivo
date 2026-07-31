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

bool TopologyBuilder::begin(std::uint32_t revision,
                            std::uint16_t debounceMs) {
  if (debounceMs == 0 || debounceMs > 1000) return false;
  pending_ = RuntimeTopology{revision, debounceMs, {}, {}};
  sourceIndices_.clear();
  ownedPins_.clear();
  return true;
}

bool TopologyBuilder::addPins(std::uint8_t sourceIndex,
                              const std::vector<std::uint8_t> &pins) {
  if (!pending_.has_value() || pins.empty() ||
      std::find(sourceIndices_.begin(), sourceIndices_.end(), sourceIndex) !=
          sourceIndices_.end()) {
    return false;
  }
  for (const auto pin : pins) {
    if (!profile_.supports(pin) ||
        std::find(ownedPins_.begin(), ownedPins_.end(), pin) !=
            ownedPins_.end() ||
        std::count(pins.begin(), pins.end(), pin) != 1) {
      return false;
    }
  }
  sourceIndices_.push_back(sourceIndex);
  ownedPins_.insert(ownedPins_.end(), pins.begin(), pins.end());
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
