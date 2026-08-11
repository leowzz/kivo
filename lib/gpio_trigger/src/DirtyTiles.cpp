#include "DirtyTiles.h"

#include <algorithm>

namespace {
constexpr std::size_t kTileBytes = 8;
constexpr std::size_t kBitmapBits = 64;
}  // namespace

RefreshMode selectRefreshMode(bool partialUpdateSupported,
                              std::uint16_t rotationDegrees) {
  return partialUpdateSupported && rotationDegrees == 0 ? RefreshMode::Tiles
                                                         : RefreshMode::Full;
}

DirtyTiles::DirtyTiles(std::uint8_t widthTiles, std::uint8_t heightTiles)
    : widthTiles_(widthTiles), heightTiles_(heightTiles) {}

void DirtyTiles::markPixels(const DisplayRect &bounds) {
  if (bounds.width == 0 || bounds.height == 0 || widthTiles_ == 0 ||
      heightTiles_ == 0) {
    return;
  }

  const auto startTx = std::min<std::uint32_t>(bounds.x / 8U, widthTiles_);
  const auto startTy = std::min<std::uint32_t>(bounds.y / 8U, heightTiles_);
  const auto endTx = std::min<std::uint32_t>(
      (static_cast<std::uint32_t>(bounds.x) + bounds.width + 7U) / 8U,
      widthTiles_);
  const auto endTy = std::min<std::uint32_t>(
      (static_cast<std::uint32_t>(bounds.y) + bounds.height + 7U) / 8U,
      heightTiles_);

  for (std::uint32_t ty = startTy; ty < endTy; ++ty) {
    for (std::uint32_t tx = startTx; tx < endTx; ++tx) {
      const auto bit = ty * widthTiles_ + tx;
      if (bit < kBitmapBits) bits_ |= std::uint64_t{1} << bit;
    }
  }
}

void DirtyTiles::clear() { bits_ = 0; }

std::optional<TileRun> DirtyTiles::takeRun(std::size_t maxDataBytes) {
  const auto maxTiles = maxDataBytes / kTileBytes;
  if (bits_ == 0 || maxTiles == 0 || widthTiles_ == 0) return std::nullopt;

  const auto tileCount = std::min<std::size_t>(
      static_cast<std::size_t>(widthTiles_) * heightTiles_, kBitmapBits);
  std::size_t first = 0;
  while (first < tileCount && (bits_ & (std::uint64_t{1} << first)) == 0) {
    ++first;
  }
  if (first == tileCount) return std::nullopt;

  const auto tx = first % widthTiles_;
  const auto ty = first / widthTiles_;
  const auto rowEnd = std::min<std::size_t>((ty + 1U) * widthTiles_, tileCount);
  std::size_t runTiles = 0;
  while (first + runTiles < rowEnd && runTiles < maxTiles &&
         (bits_ & (std::uint64_t{1} << (first + runTiles))) != 0) {
    ++runTiles;
  }

  for (std::size_t offset = 0; offset < runTiles; ++offset) {
    bits_ &= ~(std::uint64_t{1} << (first + offset));
  }
  return TileRun{static_cast<std::uint8_t>(tx),
                 static_cast<std::uint8_t>(ty),
                 static_cast<std::uint8_t>(runTiles), 1};
}
