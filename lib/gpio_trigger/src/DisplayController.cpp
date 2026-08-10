#include "DisplayController.h"

DisplayUpdate DisplayController::showLocal(const DisplayFrame &frame,
                                           LocalDisplayPriority priority) {
  local_ = frame;
  if (priority == LocalDisplayPriority::Startup) {
    if (disconnected_) {
      localOverride_ = resumeLocalOverride_;
      disconnected_ = false;
    }
    if (localOverride_) {
      source_ = DisplaySource::Local;
      return localUpdate();
    }
    if (remote_.has_value()) {
      source_ = DisplaySource::Remote;
      return remoteUpdate(true);
    }
    source_ = DisplaySource::Local;
    return localUpdate();
  }
  if (priority == LocalDisplayPriority::Critical) {
    localOverride_ = true;
    source_ = DisplaySource::Local;
    return localUpdate();
  }
  if (localOverride_ || source_ == DisplaySource::Remote) return {};
  return localUpdate();
}

DisplayUpdate DisplayController::clearLocalOverride() {
  if (!localOverride_) return {};
  localOverride_ = false;
  if (remote_.has_value()) {
    source_ = DisplaySource::Remote;
    return remoteUpdate(true);
  }
  source_ = DisplaySource::Local;
  return localUpdate();
}

DisplayUpdate DisplayController::commitRemote(
    const RemoteDisplayCommit &scene) {
  if (!remote_.has_value() && !scene.full) return {};
  remote_ = scene;
  if (localOverride_) return {};
  source_ = DisplaySource::Remote;
  return remoteUpdate(scene.full);
}

DisplayUpdate DisplayController::helperDisconnected(
    const DisplayFrame &offline) {
  if (!disconnected_) resumeLocalOverride_ = localOverride_;
  remote_.reset();
  local_ = offline;
  localOverride_ = true;
  disconnected_ = true;
  source_ = DisplaySource::Local;
  return localUpdate();
}

std::uint32_t DisplayController::remoteRevision() const {
  return remote_.has_value() ? remote_->revision : 0;
}

DisplayUpdate DisplayController::localUpdate() const {
  if (!local_.has_value()) return {};
  return {DisplayUpdateKind::Local, &*local_, nullptr, true};
}

DisplayUpdate DisplayController::remoteUpdate(bool fullRedraw) const {
  if (!remote_.has_value()) return {};
  return {DisplayUpdateKind::Remote, nullptr, &*remote_, fullRedraw};
}
