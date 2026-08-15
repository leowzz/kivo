#include "DisplayController.h"

DisplayUpdate DisplayController::showLocal(const DisplayFrame &frame,
                                           LocalDisplayPriority priority) {
  if (disconnected_) return localUpdate();
  local_ = frame;
  if (priority == LocalDisplayPriority::Critical) interactive_.reset();
  if (interactive_.has_value()) return {};
  if (priority == LocalDisplayPriority::Startup) {
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

DisplayUpdate DisplayController::showInteractive(const DisplayFrame &frame) {
  interactive_ = frame;
  source_ = DisplaySource::Local;
  return localUpdate();
}

DisplayUpdate DisplayController::clearInteractive() {
  if (!interactive_.has_value()) return {};
  interactive_.reset();
  if (disconnected_ || localOverride_ || !remote_.has_value()) {
    source_ = DisplaySource::Local;
    return localUpdate();
  }
  source_ = DisplaySource::Remote;
  return remoteUpdate(true);
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
  if (!connected_ || (!remote_.has_value() && !scene.full)) return {};
  remote_ = scene;
  if (localOverride_ || interactive_.has_value()) return {};
  source_ = DisplaySource::Remote;
  return remoteUpdate(scene.full);
}

DisplayUpdate DisplayController::helperConnected(const DisplayFrame &ready) {
  if (disconnected_) localOverride_ = resumeLocalOverride_;
  connected_ = true;
  disconnected_ = false;
  remote_.reset();
  local_ = ready;
  source_ = DisplaySource::Local;
  return localUpdate();
}

DisplayUpdate DisplayController::helperDisconnected(
    const DisplayFrame &offline) {
  if (!disconnected_) resumeLocalOverride_ = localOverride_;
  remote_.reset();
  local_ = offline;
  localOverride_ = true;
  connected_ = false;
  disconnected_ = true;
  source_ = DisplaySource::Local;
  return localUpdate();
}

DisplayUpdate DisplayController::displayReconfigured() const {
  if (interactive_.has_value()) return localUpdate();
  return source_ == DisplaySource::Remote ? remoteUpdate(true) : localUpdate();
}

DisplayUpdate DisplayController::displayFailed(const DisplayFrame &failure) {
  return showLocal(failure, LocalDisplayPriority::Critical);
}

std::uint32_t DisplayController::remoteRevision() const {
  return remote_.has_value() ? remote_->revision : 0;
}

DisplayUpdate DisplayController::localUpdate() const {
  if (interactive_.has_value()) {
    return {DisplayUpdateKind::Local, &*interactive_, nullptr, true};
  }
  if (!local_.has_value()) return {};
  return {DisplayUpdateKind::Local, &*local_, nullptr, true};
}

DisplayUpdate DisplayController::remoteUpdate(bool fullRedraw) const {
  if (!remote_.has_value()) return {};
  return {DisplayUpdateKind::Remote, nullptr, &*remote_, fullRedraw};
}
