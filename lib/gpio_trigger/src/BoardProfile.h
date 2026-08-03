#pragma once

#include <array>
#include <cstddef>
#include <cstdint>

struct BoardProfile {
  const char *controllerFamilyId;
  const char *boardProfileId;
  const std::uint8_t *safePins;
  std::size_t safePinCount;

  bool supports(std::uint8_t pin) const {
    for (std::size_t index = 0; index < safePinCount; ++index) {
      if (safePins[index] == pin) return true;
    }
    return false;
  }
};

inline constexpr std::array<std::uint8_t, 17> kEsp32S3SafePins = {
    0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 12, 13, 14, 15, 16, 17, 18};
inline constexpr std::array<std::uint8_t, 23> kYdRp2040SafePins = {
    0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11,
    12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22};

inline constexpr BoardProfile kLuatOsEsp32S3Aio = {
    "esp32s3", "luatos-esp32s3-aio", kEsp32S3SafePins.data(),
    kEsp32S3SafePins.size()};
inline constexpr BoardProfile kVccGndYdRp2040 = {
    "rp2040", "vccgnd-yd-rp2040", kYdRp2040SafePins.data(),
    kYdRp2040SafePins.size()};
