#pragma once

#include "DisplayController.h"
#include "DisplaySurface.h"
#include "TriggerProtocol.h"

class DisplayRuntime {
 public:
  explicit DisplayRuntime(DisplaySurface &surface) : surface_(surface) {}

  bool configure(const std::optional<DisplayConfig> &config,
                 const DisplayFrame &status, std::uint8_t brightness);
  void showLocal(const DisplayFrame &frame, LocalDisplayPriority priority);
  void showInteractive(const DisplayFrame &frame);
  void dismissInteractive();
  void connectionChanged(bool connected, const DisplayFrame &frame);
  std::optional<std::string> handle(const HelperCommand &command);
  std::optional<std::string> malformed(std::string_view line);
  void setBrightness(std::uint8_t percent) { surface_.setBrightness(percent); }
  std::optional<std::string> service();

 private:
  bool apply(const DisplayUpdate &update);
  DisplaySurface &surface_;
  DisplayController controller_;
  RemoteDisplay remote_;
  bool enabled_ = false;
  bool ready_ = false;
  bool connected_ = false;
  DisplayFrame status_{};
};
