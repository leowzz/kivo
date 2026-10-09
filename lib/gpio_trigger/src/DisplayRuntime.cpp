#include "DisplayRuntime.h"

bool DisplayRuntime::configure(const std::optional<DisplayConfig> &config,
                               const DisplayFrame &status,
                               std::uint8_t brightness) {
  enabled_ = config.has_value();
  remote_.reset(config ? displayCapabilities(config->panel)
                       : DisplayCapabilities{});
  status_ = status;
  ready_ = surface_.configure(config);
  // A changed panel invalidates the old remote scene and revision.
  controller_ = DisplayController{};
  if (connected_) controller_.helperConnected(status);
  else controller_.helperDisconnected(status);
  if (!ready_) {
    auto failure = status;
    failure.lines[1] = "DISPLAY ERROR";
    apply(controller_.displayFailed(failure));
    return false;
  }
  surface_.setBrightness(brightness);
  return apply(controller_.displayReconfigured());
}

bool DisplayRuntime::apply(const DisplayUpdate &update) {
  bool rendered = true;
  if (update.kind == DisplayUpdateKind::Local && update.local) {
    rendered = surface_.renderLocal(*update.local);
  } else if (update.kind == DisplayUpdateKind::Remote && update.remote) {
    rendered = surface_.renderRemote(*update.remote, update.fullRedraw);
  }
  if (!rendered) {
    auto failure = status_;
    failure.lines[1] = "DISPLAY ERROR";
    failure.lines[2].clear();
    const auto fallback = controller_.displayFailed(failure);
    if (fallback.local) surface_.renderLocal(*fallback.local);
  }
  return rendered;
}

void DisplayRuntime::showLocal(const DisplayFrame &frame,
                               LocalDisplayPriority priority) {
  status_ = frame;
  apply(controller_.showLocal(frame, priority));
}

void DisplayRuntime::showInteractive(const DisplayFrame &frame) {
  apply(controller_.showInteractive(frame));
}

void DisplayRuntime::dismissInteractive() {
  apply(controller_.clearInteractive());
}

void DisplayRuntime::connectionChanged(bool connected,
                                       const DisplayFrame &frame) {
  connected_ = connected;
  remote_.reset(remote_.capabilities());
  surface_.reset();
  status_ = frame;
  apply(connected ? controller_.helperConnected(frame)
                  : controller_.helperDisconnected(frame));
}

std::optional<std::string> DisplayRuntime::handle(const HelperCommand &command) {
  auto reply = dispatchDisplayCommand(remote_, command, enabled_ && ready_);
  if (command.kind == HelperCommandKind::DisplayCommit &&
      reply == formatDisplayOk(command.revision) && remote_.lastCommit()) {
    if (!apply(controller_.commitRemote(*remote_.lastCommit()))) {
      remote_.reset(remote_.capabilities());
      return formatDisplayError(command.revision, "render_failed");
    }
  }
  return reply;
}

std::optional<std::string> DisplayRuntime::malformed(std::string_view line) {
  return discardMalformedDisplayCommand(remote_, line);
}

std::optional<std::string> DisplayRuntime::service() {
  if (!enabled_ || !ready_ || surface_.service()) return std::nullopt;
  ready_ = false;
  return formatDisplayError(remote_.revision(), "transfer_failed");
}
