#pragma once

#include <cstdint>
#include <cstddef>

#include <Wire.h>

namespace platform {
// Cooperative leases share a hardware bus only when pin assignments match.
// The last lease releases it; an individual peripheral never calls Wire.end.
class I2cBus {
 public:
  I2cBus() = default;
  ~I2cBus() { release(); }
  I2cBus(const I2cBus &) = delete;
  I2cBus &operator=(const I2cBus &) = delete;
  bool acquire(std::uint8_t sda, std::uint8_t scl);
  void release();
  bool healthy() const { return healthy_; }
  bool software() const { return wire_ == nullptr; }
  bool beginTransmission(std::uint8_t address);
  bool write(const std::uint8_t *data, std::size_t size);
  bool endTransmission();

 private:
  TwoWire *wire_ = nullptr;
  bool acquired_ = false;
  bool healthy_ = true;
};
}  // namespace platform
