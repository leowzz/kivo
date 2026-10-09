#pragma once

#include <cstdint>

namespace platform {
enum class Rp2040I2cRouting { Software, I2c0, I2c1 };

constexpr Rp2040I2cRouting selectRp2040I2cRouting(std::uint8_t sda,
                                            std::uint8_t scl) {
  if (sda <= 28 && scl <= 29 && sda % 4 == 0 && scl % 4 == 1) {
    return Rp2040I2cRouting::I2c0;
  }
  if (sda <= 26 && scl <= 27 && sda % 4 == 2 && scl % 4 == 3) {
    return Rp2040I2cRouting::I2c1;
  }
  return Rp2040I2cRouting::Software;
}
}  // namespace platform
