#pragma once

#include <array>
#include <cstddef>
#include <cstdint>
#include <optional>

#include "RemoteDisplay.h"

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
  bool hasDirty() const;
  std::optional<TileRun> takeRun(std::size_t maxDataBytes);

 private:
  std::uint8_t widthTiles_;
  std::uint8_t heightTiles_;
  std::array<std::uint64_t, 2> bits_{};
};
