#pragma once

#include <cstddef>
#include <cstdint>
#include <optional>

#include "RemoteDisplay.h"

enum class RefreshMode : std::uint8_t { Full, Tiles };

RefreshMode selectRefreshMode(bool partialUpdateSupported,
                              std::uint16_t rotationDegrees);

struct TileRun {
  std::uint8_t tx = 0;
  std::uint8_t ty = 0;
  std::uint8_t tw = 0;
  std::uint8_t th = 0;

  std::size_t dataBytes() const {
    return static_cast<std::size_t>(tw) * 8U * th;
  }
};

class DirtyTiles {
 public:
  DirtyTiles(std::uint8_t widthTiles, std::uint8_t heightTiles);

  void markPixels(const DisplayRect &bounds);
  void clear();
  bool hasDirty() const { return bits_ != 0; }
  std::optional<TileRun> takeRun(std::size_t maxDataBytes);

 private:
  std::uint8_t widthTiles_;
  std::uint8_t heightTiles_;
  std::uint64_t bits_ = 0;
};
