#include "RunCat.h"
#include "RunCatSprites.h"

#include <algorithm>

namespace {
const RunCatSprites &sprites(RunCatAnimation animation) {
  const auto index = static_cast<unsigned>(animation);
  return kRunCatSprites[index < static_cast<unsigned>(RunCatAnimation::Count) ? index : 0];
}
}

std::uint32_t runCatFrameInterval(std::optional<std::uint8_t> cpuPercent) {
  // Keep the larger frame sequences smooth: 8 to 20 frames per second.
  return 125U - std::min<std::uint32_t>(cpuPercent.value_or(0), 100) * 75U / 100U;
}

std::uint8_t runCatPhaseCount(RunCatAnimation animation) {
  return sprites(animation).count;
}

const RunCatBitmap &runCatBitmap(RunCatAnimation animation, std::uint8_t phase) {
  const auto &runner = sprites(animation);
  return runner.frames[phase % runner.count];
}
