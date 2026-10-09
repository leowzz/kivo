#pragma once

#include <U8g2lib.h>

#include "DirtyTiles.h"
#include "DisplaySurface.h"
#include "platform/I2cBus.h"

class U8g2Display final : public DisplaySurface {
 public:
  bool configure(const std::optional<DisplayConfig> &config) override;
  bool renderLocal(const DisplayFrame &frame) override;
  bool renderRemote(const RemoteDisplayCommit &scene, bool full) override;
  void reset() override;
  void setBrightness(std::uint8_t percent) override;
  bool service() override;

 private:
  static std::uint8_t transfer(u8x8_t *screen, std::uint8_t message,
                               std::uint8_t size, void *data);
  void stop();
  U8G2 screen_;
  platform::I2cBus bus_;
  DisplayCapabilities capabilities_{};
  DirtyTiles dirty_{16, 8};
  std::optional<DisplayFrame> lastLocal_;
  enum class Source { None, Local, Remote };
  Source source_ = Source::None;
  bool requested_ = false;
  bool ready_ = false;
};
