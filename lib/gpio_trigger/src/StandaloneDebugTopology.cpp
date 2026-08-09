#include "StandaloneDebugTopology.h"

#include <array>

namespace {
constexpr std::uint16_t kDebounceMs = 30;
constexpr std::array<std::uint8_t, 18> kInputPins = {
    1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18};
constexpr OledConfig kOled = {28, 29};
}  // namespace

std::optional<RuntimeTopology> makeRp2040StandaloneDebugTopology(
    const BoardProfile &profile) {
  constexpr std::uint32_t kRevision = 0;
  TopologyBuilder builder(profile);
  if (!builder.begin(kRevision, kDebounceMs) ||
      !builder.addDirect(
          kRevision, 0,
          std::vector<std::uint8_t>(kInputPins.begin(), kInputPins.end())) ||
      !builder.addOled(kRevision, kOled.sda, kOled.scl)) {
    return std::nullopt;
  }
  return builder.commit(kRevision);
}
