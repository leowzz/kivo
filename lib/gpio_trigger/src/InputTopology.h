#pragma once

#include <cstdint>
#include <optional>
#include <vector>

#include "BoardProfile.h"

enum class PhysicalInputKind {
  Direct,
  Contact,
};

struct PhysicalInput {
  PhysicalInputKind kind;
  std::uint8_t sourceIndex;
  std::uint8_t pinA;
  std::uint8_t pinB;

  static PhysicalInput direct(std::uint8_t gpio);
  static PhysicalInput contact(std::uint8_t sourceIndex, std::uint8_t pinA,
                               std::uint8_t pinB);
  bool operator==(const PhysicalInput &other) const;
};

struct DirectInputSource {
  std::uint8_t index;
  std::vector<std::uint8_t> pins;
};

struct MatrixInputSource {
  std::uint8_t index;
  std::vector<std::uint8_t> rows;
  std::vector<std::uint8_t> columns;
};

struct OledConfig {
  std::uint8_t sda;
  std::uint8_t scl;
};

struct OledControlPanelConfig {
  std::uint8_t confirm;
  std::uint8_t encoderPress;
  std::uint8_t encoderA;
  std::uint8_t encoderB;
  std::uint8_t back;
};

struct RuntimeTopology {
  std::uint32_t revision = 0;
  std::uint16_t debounceMs = 30;
  std::vector<DirectInputSource> directs;
  std::vector<MatrixInputSource> matrices;
  std::optional<OledConfig> oled;
  std::optional<OledControlPanelConfig> oledControlPanel;

  std::size_t keyCount() const;
};

class TopologyBuilder {
 public:
  explicit TopologyBuilder(const BoardProfile &profile) : profile_(profile) {}

  bool begin(std::uint32_t revision, std::uint16_t debounceMs);
  bool addDirect(std::uint32_t revision, std::uint8_t sourceIndex,
                 std::vector<std::uint8_t> pins);
  bool addMatrix(std::uint32_t revision, std::uint8_t sourceIndex,
                 std::vector<std::uint8_t> rows,
                 std::vector<std::uint8_t> columns);
  bool addOled(std::uint32_t revision, std::uint8_t sda,
               std::uint8_t scl);
  bool addOledControlPanel(std::uint32_t revision, std::uint8_t confirm,
                           std::uint8_t encoderPress,
                           std::uint8_t encoderA, std::uint8_t encoderB,
                           std::uint8_t back);
  std::optional<RuntimeTopology> commit(std::uint32_t revision);
  void cancel();

 private:
  bool pinsAvailable(const std::vector<std::uint8_t> &pins) const;
  bool addPins(std::uint8_t sourceIndex,
               const std::vector<std::uint8_t> &pins);

  std::optional<RuntimeTopology> pending_;
  const BoardProfile &profile_;
  std::vector<std::uint8_t> sourceIndices_;
  std::vector<std::uint8_t> ownedPins_;
};
