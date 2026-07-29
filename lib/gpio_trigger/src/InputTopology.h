#pragma once

#include <array>
#include <cstdint>
#include <optional>
#include <vector>

constexpr std::array<std::uint8_t, 17> kEsp32S3SafePins = {
    0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 12, 13, 14, 15, 16, 17, 18};

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

struct RuntimeTopology {
  std::uint32_t revision = 0;
  std::uint16_t debounceMs = 30;
  std::vector<DirectInputSource> directs;
  std::vector<MatrixInputSource> matrices;
};

class TopologyBuilder {
 public:
  bool begin(std::uint32_t revision, std::uint16_t debounceMs);
  bool addDirect(std::uint32_t revision, std::uint8_t sourceIndex,
                 std::vector<std::uint8_t> pins);
  bool addMatrix(std::uint32_t revision, std::uint8_t sourceIndex,
                 std::vector<std::uint8_t> rows,
                 std::vector<std::uint8_t> columns);
  std::optional<RuntimeTopology> commit(std::uint32_t revision);
  void cancel();

 private:
  bool addPins(std::uint8_t sourceIndex,
               const std::vector<std::uint8_t> &pins);

  std::optional<RuntimeTopology> pending_;
  std::vector<std::uint8_t> sourceIndices_;
  std::vector<std::uint8_t> ownedPins_;
};
