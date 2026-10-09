#pragma once

#include <optional>

#include "DisplayConfig.h"
#include "DisplayStatus.h"
#include "RemoteDisplay.h"

// A surface owns the framebuffer and bounded refresh work. configure releases
// its old bus lease before acquiring the new one; submit never flushes a frame.
class DisplaySurface {
 public:
  virtual ~DisplaySurface() = default;
  virtual bool configure(const std::optional<DisplayConfig> &config) = 0;
  virtual bool renderLocal(const DisplayFrame &frame) = 0;
  virtual bool renderRemote(const RemoteDisplayCommit &scene, bool full) = 0;
  virtual void reset() = 0;
  virtual void setBrightness(std::uint8_t percent) = 0;
  virtual bool service() = 0;
};
