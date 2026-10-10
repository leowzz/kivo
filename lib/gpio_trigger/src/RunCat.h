#pragma once

#include <array>
#include <cstdint>
#include <optional>

enum class RunCatAnimation : std::uint8_t {
  Cat, Rabbit, Dog, Bird, Horse, Dinosaur, WankelEngine, Escapement,
  DoubleCrank, CowAbduction, Count
};

struct RunCatFrame {
  RunCatAnimation animation = RunCatAnimation::Cat;
  std::uint8_t phase = 0;
  std::optional<std::uint8_t> cpuPercent;

  bool operator==(const RunCatFrame &other) const {
    return animation == other.animation && phase == other.phase &&
           cpuPercent == other.cpuPercent;
  }
};

std::uint32_t runCatFrameInterval(std::optional<std::uint8_t> cpuPercent);
constexpr unsigned kRunCatWidth = 96;
constexpr unsigned kRunCatHeight = 64;
constexpr unsigned kRunCatStride = kRunCatWidth / 8;
// Row-major, MSB first. The original keyframes are baked for the panel.
using RunCatBitmap = std::array<std::uint8_t, kRunCatStride * kRunCatHeight>;
std::uint8_t runCatPhaseCount(RunCatAnimation animation);
const RunCatBitmap &runCatBitmap(RunCatAnimation animation, std::uint8_t phase);
