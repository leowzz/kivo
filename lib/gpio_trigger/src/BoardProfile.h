#pragma once

#include <array>
#include <cstddef>
#include <cstdint>

struct BoardProfile {
  const char *controllerFamilyId;
  const char *boardProfileId;
  const std::uint8_t *safePins;
  std::size_t safePinCount;
  bool supportsOled;

  bool supports(std::uint8_t pin) const {
    for (std::size_t index = 0; index < safePinCount; ++index) {
      if (safePins[index] == pin) return true;
    }
    return false;
  }
};

inline constexpr std::array<std::uint8_t, 26> kYdEsp32S3SafePins = {
    0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12,
    13, 14, 15, 16, 17, 18, 21, 38, 39, 40, 41, 42, 47};
inline constexpr std::array<std::uint8_t, 27> kYdRp2040SafePins = {
    0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11,
    12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 26,
    27, 28, 29};

inline constexpr BoardProfile kYdEsp32S3 = {
    "esp32s3", "yd-esp32-s3", kYdEsp32S3SafePins.data(),
    kYdEsp32S3SafePins.size(), false};
inline constexpr BoardProfile kYdRp2040 = {
    "rp2040", "yd-rp2040", kYdRp2040SafePins.data(),
    kYdRp2040SafePins.size(), true};
