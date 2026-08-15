#pragma once

#include <cstdint>
#include <optional>

#include "DisplayStatus.h"
#include "RemoteDisplay.h"

enum class DisplaySource { Local, Remote };
enum class LocalDisplayPriority { Startup, Normal, Critical };
enum class DisplayUpdateKind { None, Local, Remote };

struct DisplayUpdate {
  DisplayUpdateKind kind = DisplayUpdateKind::None;
  const DisplayFrame *local = nullptr;
  const RemoteDisplayCommit *remote = nullptr;
  bool fullRedraw = false;
};

class DisplayController {
 public:
  DisplayUpdate showLocal(const DisplayFrame &frame,
                          LocalDisplayPriority priority);
  DisplayUpdate showInteractive(const DisplayFrame &frame);
  DisplayUpdate clearInteractive();
  DisplayUpdate clearLocalOverride();
  DisplayUpdate commitRemote(const RemoteDisplayCommit &scene);
  DisplayUpdate helperConnected(const DisplayFrame &ready);
  DisplayUpdate helperDisconnected(const DisplayFrame &offline);
  DisplayUpdate displayReconfigured() const;
  DisplayUpdate displayFailed(const DisplayFrame &failure);

  DisplaySource source() const { return source_; }
  bool hasRemote() const { return remote_.has_value(); }
  std::uint32_t remoteRevision() const;

 private:
  DisplayUpdate localUpdate() const;
  DisplayUpdate remoteUpdate(bool fullRedraw) const;

  DisplaySource source_ = DisplaySource::Local;
  bool localOverride_ = false;
  bool connected_ = true;
  bool disconnected_ = false;
  bool resumeLocalOverride_ = false;
  std::optional<DisplayFrame> local_;
  std::optional<DisplayFrame> interactive_;
  std::optional<RemoteDisplayCommit> remote_;
};
