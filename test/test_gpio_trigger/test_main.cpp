#include <unity.h>

#include <array>
#include <cstdint>
#include <vector>

#include "BoardProfile.h"
#include "DisplayController.h"
#include "DisplayStatus.h"
#include "DirtyTiles.h"
#include "ActionRunDispatcher.h"
#include "ActionRunController.h"
#include "GpioTriggerController.h"
#include "Handshake.h"
#include "InputTopology.h"
#include "KeyActivityIndicator.h"
#include "OledControlPanel.h"
#include "RemoteDisplay.h"
#include "StandaloneDebugTopology.h"
#include "TriggerProtocol.h"
#include "platform/HidReportTransport.h"
#include "platform/Rp2040OledBus.h"

static_assert(sizeof(DisplayRegionState) <= 16,
              "display regions must remain compact metadata");
static_assert(sizeof(RemoteDisplayScene) <= 1200,
              "display scenes must own only 24 operations");
static_assert(sizeof(RemoteDisplayCommit) <= 1280,
              "display commits must own only 24 operations");

void setUp() {}
void tearDown() {}

void test_dirty_tiles_emit_only_changed_counter_region() {
  DirtyTiles dirty(16, 8);
  dirty.markPixels({64, 0, 64, 16});

  std::size_t bytes = 0;
  while (const auto run = dirty.takeRun(64)) bytes += run->dataBytes();

  TEST_ASSERT_EQUAL_UINT32(128, bytes);
}

void test_dirty_tiles_respect_per_loop_budget_and_coalesce_updates() {
  DirtyTiles dirty(16, 8);
  dirty.markPixels({0, 0, 16, 8});
  dirty.markPixels({32, 0, 16, 8});
  dirty.markPixels({8, 0, 32, 8});

  const auto first = dirty.takeRun(32);
  TEST_ASSERT_TRUE(first.has_value());
  TEST_ASSERT_EQUAL_UINT8(0, first->tx);
  TEST_ASSERT_EQUAL_UINT8(0, first->ty);
  TEST_ASSERT_EQUAL_UINT8(4, first->tw);
  TEST_ASSERT_EQUAL_UINT8(1, first->th);
  TEST_ASSERT_EQUAL_UINT32(32, first->dataBytes());

  const auto second = dirty.takeRun(32);
  TEST_ASSERT_TRUE(second.has_value());
  TEST_ASSERT_EQUAL_UINT8(4, second->tx);
  TEST_ASSERT_EQUAL_UINT8(0, second->ty);
  TEST_ASSERT_EQUAL_UINT8(2, second->tw);
  TEST_ASSERT_EQUAL_UINT8(1, second->th);
  TEST_ASSERT_EQUAL_UINT32(16, second->dataBytes());
  TEST_ASSERT_FALSE(dirty.takeRun(32).has_value());
}

void test_dirty_tiles_round_outward_clip_and_stay_within_one_row() {
  DirtyTiles dirty(16, 8);
  dirty.markPixels({63, 7, 10, 3});
  dirty.markPixels({127, 63, 8, 8});

  const auto first = dirty.takeRun(512);
  TEST_ASSERT_TRUE(first.has_value());
  TEST_ASSERT_EQUAL_UINT8(7, first->tx);
  TEST_ASSERT_EQUAL_UINT8(0, first->ty);
  TEST_ASSERT_EQUAL_UINT8(3, first->tw);
  TEST_ASSERT_EQUAL_UINT8(1, first->th);

  const auto second = dirty.takeRun(512);
  TEST_ASSERT_TRUE(second.has_value());
  TEST_ASSERT_EQUAL_UINT8(7, second->tx);
  TEST_ASSERT_EQUAL_UINT8(1, second->ty);
  TEST_ASSERT_EQUAL_UINT8(3, second->tw);
  TEST_ASSERT_EQUAL_UINT8(1, second->th);

  const auto third = dirty.takeRun(512);
  TEST_ASSERT_TRUE(third.has_value());
  TEST_ASSERT_EQUAL_UINT8(15, third->tx);
  TEST_ASSERT_EQUAL_UINT8(7, third->ty);
  TEST_ASSERT_EQUAL_UINT8(1, third->tw);
  TEST_ASSERT_EQUAL_UINT8(1, third->th);
  TEST_ASSERT_FALSE(dirty.takeRun(512).has_value());
}

void test_dirty_tiles_reject_sub_tile_budget_and_clear_explicitly() {
  DirtyTiles dirty(16, 8);
  dirty.markPixels({0, 0, 128, 64});

  TEST_ASSERT_FALSE(dirty.takeRun(7).has_value());
  TEST_ASSERT_TRUE(dirty.hasDirty());

  std::size_t bytes = 0;
  while (const auto run = dirty.takeRun(64)) bytes += run->dataBytes();
  TEST_ASSERT_EQUAL_UINT32(1024, bytes);
  TEST_ASSERT_FALSE(dirty.hasDirty());

  dirty.markPixels({0, 0, 8, 8});
  dirty.clear();
  TEST_ASSERT_FALSE(dirty.hasDirty());
}

void test_rotated_or_unsupported_panel_requests_full_refresh() {
  TEST_ASSERT_EQUAL(RefreshMode::Full, selectRefreshMode(false, 0));
  TEST_ASSERT_EQUAL(RefreshMode::Full, selectRefreshMode(true, 90));
  TEST_ASSERT_EQUAL(RefreshMode::Tiles, selectRefreshMode(true, 0));
}

GpioTriggerController directController(std::uint32_t startMs) {
  TopologyBuilder builder(kYdEsp32S3);
  builder.begin(1, 30);
  builder.addDirect(1, 0, {0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 12, 13, 14, 15, 16,
                           17, 18});
  GpioTriggerController controller(kYdEsp32S3, startMs);
  controller.configure(*builder.commit(1), startMs);
  return controller;
}

std::vector<platform::KeyboardReport> captureKeyboardReports(
    std::uint8_t modifiers,
    const std::array<std::uint8_t, 6> &keys) {
  std::vector<platform::KeyboardReport> reports;
  const bool sent = platform::transmitKeyboardReports(
      modifiers, keys, 0, []() { return true; },
      [&reports](const platform::KeyboardReport &report) {
        reports.push_back(report);
        return true;
      },
      []() {});
  TEST_ASSERT_TRUE(sent);
  return reports;
}

const DisplayOperation *displayTextOperation(const RemoteDisplayScene &scene,
                                             std::uint8_t slot,
                                             std::size_t textIndex = 0) {
  for (std::size_t index = 0; index < scene.operationCount; ++index) {
    const auto &operation = scene.operations[index];
    if (operation.slot == slot &&
        operation.kind == DisplayOperationKind::Text) {
      if (textIndex == 0) return &operation;
      --textIndex;
    }
  }
  return nullptr;
}

DisplayFrame localFrame(const char *status) {
  return DisplayFrame{{"KIVO", status, ""}};
}

RemoteDisplayCommit remoteScene(std::uint32_t revision, const char *first,
                                const char *second, bool full = true) {
  RemoteDisplayCommit scene;
  scene.revision = revision;
  scene.full = full;
  scene.regions[0] = {0, {0, 0, 128, 32}};
  scene.regions[1] = {1, {0, 32, 128, 32}};
  scene.regionCount = 2;
  scene.operations[0] = {"", 0, 0, 0, 0, DisplayOperationKind::Clear};
  scene.operations[1] = {first, 0, 23, 0, 0,
                         DisplayOperationKind::Text};
  scene.operations[2] = {"", 0, 0, 1, 0, DisplayOperationKind::Clear};
  scene.operations[3] = {second, 0, 55, 1, 0,
                         DisplayOperationKind::Text};
  scene.operationCount = 4;
  scene.dirtyBounds[0] = {0, 0, 128, 64};
  scene.dirtyCount = 1;
  return scene;
}

void test_startup_stays_local_until_first_full_remote_scene() {
  DisplayController controller;

  const auto startup =
      controller.showLocal(localFrame("WAITING CONFIG"),
                           LocalDisplayPriority::Startup);
  TEST_ASSERT_EQUAL(DisplayUpdateKind::Local, startup.kind);
  TEST_ASSERT_EQUAL(DisplaySource::Local, controller.source());

  const auto ignoredDelta =
      controller.commitRemote(remoteScene(1, "CODEX", "1 RUN", false));
  TEST_ASSERT_EQUAL(DisplayUpdateKind::None, ignoredDelta.kind);
  TEST_ASSERT_FALSE(controller.hasRemote());
  TEST_ASSERT_EQUAL(DisplaySource::Local, controller.source());

  const auto firstFull =
      controller.commitRemote(remoteScene(2, "CODEX", "1 RUN"));
  TEST_ASSERT_EQUAL(DisplayUpdateKind::Remote, firstFull.kind);
  TEST_ASSERT_TRUE(firstFull.fullRedraw);
  TEST_ASSERT_NOT_NULL(firstFull.remote);
  TEST_ASSERT_EQUAL_UINT32(2, firstFull.remote->revision);
  TEST_ASSERT_EQUAL(DisplaySource::Remote, controller.source());
}

void test_startup_refresh_does_not_demote_remote_or_critical_content() {
  DisplayController controller;
  controller.commitRemote(remoteScene(1, "CODEX", "RUNNING"));

  const auto remoteRefresh = controller.showLocal(
      localFrame("WAITING CONFIG"), LocalDisplayPriority::Startup);
  TEST_ASSERT_EQUAL(DisplayUpdateKind::Remote, remoteRefresh.kind);
  TEST_ASSERT_TRUE(remoteRefresh.fullRedraw);
  TEST_ASSERT_EQUAL(DisplaySource::Remote, controller.source());

  controller.showLocal(localFrame("CONFIG ERROR"),
                       LocalDisplayPriority::Critical);
  const auto criticalRefresh = controller.showLocal(
      localFrame("CONFIG ERROR"), LocalDisplayPriority::Startup);
  TEST_ASSERT_EQUAL(DisplayUpdateKind::Local, criticalRefresh.kind);
  TEST_ASSERT_EQUAL(DisplaySource::Local, controller.source());
  const auto hiddenDelta =
      controller.commitRemote(remoteScene(2, "CODEX", "READY", false));
  TEST_ASSERT_EQUAL(DisplayUpdateKind::None, hiddenDelta.kind);
}

void test_display_reconfiguration_redraws_the_current_visible_source() {
  DisplayController controller;
  controller.helperConnected(localFrame("READY 9 KEYS"));
  controller.commitRemote(remoteScene(11, "CODEX", "RUNNING"));

  const auto remoteRedraw = controller.displayReconfigured();

  TEST_ASSERT_EQUAL(DisplayUpdateKind::Remote, remoteRedraw.kind);
  TEST_ASSERT_TRUE(remoteRedraw.fullRedraw);
  TEST_ASSERT_NOT_NULL(remoteRedraw.remote);
  TEST_ASSERT_EQUAL_UINT32(11, remoteRedraw.remote->revision);
  TEST_ASSERT_EQUAL(DisplaySource::Remote, controller.source());

  controller.displayFailed(localFrame("DISPLAY ERROR"));
  const auto criticalRedraw = controller.displayReconfigured();
  TEST_ASSERT_EQUAL(DisplayUpdateKind::Local, criticalRedraw.kind);
  TEST_ASSERT_EQUAL(DisplaySource::Local, controller.source());
}

void test_local_critical_overrides_and_then_restores_latest_remote_scene() {
  DisplayController controller;
  const auto remote1 = remoteScene(1, "CODEX", "1 RUN");
  const auto remote2 = remoteScene(2, "KIVO", "NEEDS INPUT", false);
  controller.commitRemote(remote1);
  TEST_ASSERT_EQUAL(DisplaySource::Remote, controller.source());

  const auto critical = controller.showLocal(
      localFrame("CONFIG ERROR"), LocalDisplayPriority::Critical);
  TEST_ASSERT_EQUAL(DisplayUpdateKind::Local, critical.kind);
  TEST_ASSERT_EQUAL(DisplaySource::Local, controller.source());

  const auto hiddenRemote = controller.commitRemote(remote2);
  TEST_ASSERT_EQUAL(DisplayUpdateKind::None, hiddenRemote.kind);
  TEST_ASSERT_EQUAL(DisplaySource::Local, controller.source());

  const auto restored = controller.clearLocalOverride();
  TEST_ASSERT_EQUAL(DisplayUpdateKind::Remote, restored.kind);
  TEST_ASSERT_TRUE(restored.fullRedraw);
  TEST_ASSERT_NOT_NULL(restored.remote);
  TEST_ASSERT_EQUAL_UINT32(2, restored.remote->revision);
  TEST_ASSERT_EQUAL(DisplaySource::Remote, controller.source());
  TEST_ASSERT_EQUAL_UINT32(2, controller.remoteRevision());
}

void test_learning_override_retains_remote_and_restores_on_runtime_return() {
  DisplayController controller;
  controller.commitRemote(remoteScene(7, "CODEX", "IDLE"));

  controller.showLocal(localFrame("LEARNING 4 PINS"),
                       LocalDisplayPriority::Critical);
  controller.showLocal(localFrame("LEARNING GPIO 6"),
                       LocalDisplayPriority::Critical);
  controller.commitRemote(remoteScene(8, "KIVO", "TASK STOPPED", false));

  TEST_ASSERT_EQUAL(DisplaySource::Local, controller.source());
  const auto restored = controller.clearLocalOverride();
  TEST_ASSERT_EQUAL(DisplayUpdateKind::Remote, restored.kind);
  TEST_ASSERT_TRUE(restored.fullRedraw);
  TEST_ASSERT_EQUAL_UINT32(8, restored.remote->revision);
}

void test_normal_input_debug_does_not_overwrite_active_remote_scene() {
  DisplayController controller;
  controller.commitRemote(remoteScene(3, "CODEX", "2 RUN"));

  const auto inputUpdate =
      controller.showLocal(DisplayFrame{{"KIVO", "READY 9 KEYS", "6 D"}},
                           LocalDisplayPriority::Normal);

  TEST_ASSERT_EQUAL(DisplayUpdateKind::None, inputUpdate.kind);
  TEST_ASSERT_EQUAL(DisplaySource::Remote, controller.source());
  TEST_ASSERT_EQUAL_UINT32(3, controller.remoteRevision());
}

void test_disconnect_discards_remote_and_reconnect_requires_new_full_scene() {
  DisplayController controller;
  controller.commitRemote(remoteScene(4, "CODEX", "2 RUN"));

  const auto disconnected =
      controller.helperDisconnected(localFrame("HELPER OFFLINE"));
  TEST_ASSERT_EQUAL(DisplayUpdateKind::Local, disconnected.kind);
  TEST_ASSERT_EQUAL(DisplaySource::Local, controller.source());
  TEST_ASSERT_FALSE(controller.hasRemote());
  TEST_ASSERT_EQUAL_UINT32(0, controller.remoteRevision());

  controller.helperConnected(localFrame("READY 9 KEYS"));
  const auto reconnectDelta =
      controller.commitRemote(remoteScene(5, "CODEX", "STALE", false));
  TEST_ASSERT_EQUAL(DisplayUpdateKind::None, reconnectDelta.kind);
  TEST_ASSERT_FALSE(controller.hasRemote());
  TEST_ASSERT_EQUAL(DisplaySource::Local, controller.source());

  const auto reconnectFull =
      controller.commitRemote(remoteScene(6, "CODEX", "READY"));
  TEST_ASSERT_EQUAL(DisplayUpdateKind::Remote, reconnectFull.kind);
  TEST_ASSERT_TRUE(controller.hasRemote());
  TEST_ASSERT_EQUAL_UINT32(6, controller.remoteRevision());
}

void test_reconnect_preserves_the_critical_override_from_before_disconnect() {
  DisplayController controller;
  controller.commitRemote(remoteScene(1, "CODEX", "RUNNING"));
  controller.showLocal(localFrame("LEARNING 4 PINS"),
                       LocalDisplayPriority::Critical);
  controller.helperDisconnected(localFrame("HELPER OFFLINE"));

  controller.helperConnected(localFrame("LEARNING 4 PINS"));
  const auto hiddenFull =
      controller.commitRemote(remoteScene(2, "CODEX", "READY"));

  TEST_ASSERT_EQUAL(DisplayUpdateKind::None, hiddenFull.kind);
  TEST_ASSERT_EQUAL(DisplaySource::Local, controller.source());
  const auto restored = controller.clearLocalOverride();
  TEST_ASSERT_EQUAL(DisplayUpdateKind::Remote, restored.kind);
  TEST_ASSERT_EQUAL_UINT32(2, restored.remote->revision);
}

void test_disconnected_remote_commit_is_discarded_before_reconnect_full() {
  DisplayController controller;
  controller.commitRemote(remoteScene(4, "CODEX", "RUNNING"));
  controller.helperDisconnected(localFrame("HELPER OFFLINE"));

  const auto disconnectedCommit =
      controller.commitRemote(remoteScene(5, "CODEX", "STALE"));

  TEST_ASSERT_EQUAL(DisplayUpdateKind::None, disconnectedCommit.kind);
  TEST_ASSERT_FALSE(controller.hasRemote());
  TEST_ASSERT_EQUAL_UINT32(0, controller.remoteRevision());

  controller.helperConnected(localFrame("READY 9 KEYS"));
  const auto reconnectDelta =
      controller.commitRemote(remoteScene(6, "CODEX", "DELTA", false));
  TEST_ASSERT_EQUAL(DisplayUpdateKind::None, reconnectDelta.kind);
  TEST_ASSERT_FALSE(controller.hasRemote());

  const auto reconnectFull =
      controller.commitRemote(remoteScene(7, "CODEX", "FRESH"));
  TEST_ASSERT_EQUAL(DisplayUpdateKind::Remote, reconnectFull.kind);
  TEST_ASSERT_EQUAL_UINT32(7, controller.remoteRevision());
}

void test_delayed_display_reconfiguration_preserves_offline_after_disconnect() {
  DisplayController controller;
  controller.helperConnected(localFrame("READY 9 KEYS"));
  controller.helperDisconnected(localFrame("HELPER OFFLINE"));

  const auto delayedStartup = controller.showLocal(
      localFrame("READY 9 KEYS"), LocalDisplayPriority::Startup);
  const auto delayedReconfigure = controller.displayReconfigured();

  TEST_ASSERT_EQUAL(DisplayUpdateKind::Local, delayedStartup.kind);
  TEST_ASSERT_NOT_NULL(delayedStartup.local);
  TEST_ASSERT_EQUAL_STRING("HELPER OFFLINE",
                           delayedStartup.local->lines[1].c_str());
  TEST_ASSERT_EQUAL(DisplayUpdateKind::Local, delayedReconfigure.kind);
  TEST_ASSERT_NOT_NULL(delayedReconfigure.local);
  TEST_ASSERT_EQUAL_STRING("HELPER OFFLINE",
                           delayedReconfigure.local->lines[1].c_str());
  TEST_ASSERT_EQUAL(DisplaySource::Local, controller.source());

  const auto disconnectedFull =
      controller.commitRemote(remoteScene(8, "CODEX", "STALE"));
  TEST_ASSERT_EQUAL(DisplayUpdateKind::None, disconnectedFull.kind);
  TEST_ASSERT_FALSE(controller.hasRemote());

  controller.helperConnected(localFrame("READY 9 KEYS"));
  const auto reconnectFull =
      controller.commitRemote(remoteScene(9, "CODEX", "FRESH"));
  TEST_ASSERT_EQUAL(DisplayUpdateKind::Remote, reconnectFull.kind);
  TEST_ASSERT_EQUAL_UINT32(9, controller.remoteRevision());
}

void test_interactive_panel_restores_the_latest_remote_scene() {
  DisplayController controller;
  controller.commitRemote(remoteScene(1, "CODEX", "1 RUN"));

  const auto menu = controller.showInteractive(
      DisplayFrame{{"KIVO MENU", "> LIVE VIEW", "  SYSTEM STATUS", "  INPUT TEST"}});
  TEST_ASSERT_EQUAL(DisplayUpdateKind::Local, menu.kind);
  TEST_ASSERT_EQUAL(DisplaySource::Local, controller.source());

  const auto hidden =
      controller.commitRemote(remoteScene(2, "KIVO", "NEEDS INPUT", false));
  TEST_ASSERT_EQUAL(DisplayUpdateKind::None, hidden.kind);

  const auto restored = controller.clearInteractive();
  TEST_ASSERT_EQUAL(DisplayUpdateKind::Remote, restored.kind);
  TEST_ASSERT_TRUE(restored.fullRedraw);
  TEST_ASSERT_EQUAL_UINT32(2, restored.remote->revision);
}

void test_interactive_panel_returns_to_offline_status_without_remote_content() {
  DisplayController controller;
  controller.helperDisconnected(localFrame("HELPER OFFLINE"));
  controller.showInteractive(
      DisplayFrame{{"DEVICE INFO", "SH1106 128X64", "I2C 0X3C", "EC11"}});

  const auto restored = controller.clearInteractive();

  TEST_ASSERT_EQUAL(DisplayUpdateKind::Local, restored.kind);
  TEST_ASSERT_NOT_NULL(restored.local);
  TEST_ASSERT_EQUAL_STRING("HELPER OFFLINE", restored.local->lines[1].c_str());
}

OledControlPanelUpdate rotateOledEncoder(OledControlPanel &panel,
                                         OledControlPanelSample &sample,
                                         std::uint32_t &nowMs,
                                         bool clockwise);

void test_oled_control_panel_navigates_status_and_back_to_live_view() {
  OledControlPanel panel;
  OledControlPanelSample sample;
  const auto status = DisplayFrame{{"KIVO USB ON", "READY 18 KEYS", "12 D", ""}};

  TEST_ASSERT_EQUAL(OledControlPanelUpdate::None,
                    panel.update(sample, 0, 10));
  sample.encoderPressed = true;
  TEST_ASSERT_EQUAL(OledControlPanelUpdate::None,
                    panel.update(sample, 1, 10));
  TEST_ASSERT_EQUAL(OledControlPanelUpdate::Render,
                    panel.update(sample, 11, 10));
  TEST_ASSERT_TRUE(panel.active());
  TEST_ASSERT_EQUAL_STRING("> LIVE VIEW", panel.frame(status).lines[1].c_str());

  sample.encoderPressed = false;
  panel.update(sample, 12, 10);
  panel.update(sample, 22, 10);
  std::uint32_t nowMs = 22;
  for (int step = 0; step < 2; ++step) {
    TEST_ASSERT_EQUAL(OledControlPanelUpdate::Render,
                      rotateOledEncoder(panel, sample, nowMs, true));
  }
  TEST_ASSERT_EQUAL_STRING("> SYSTEM STATUS",
                           panel.frame(status).lines[3].c_str());

  sample.confirmPressed = true;
  panel.update(sample, nowMs + 20, 10);
  TEST_ASSERT_EQUAL(OledControlPanelUpdate::Render,
                    panel.update(sample, nowMs + 30, 10));
  TEST_ASSERT_EQUAL_STRING("SYSTEM STATUS",
                           panel.frame(status).lines[0].c_str());
  TEST_ASSERT_EQUAL_STRING("12 D", panel.frame(status).lines[3].c_str());

  sample.confirmPressed = false;
  panel.update(sample, nowMs + 31, 10);
  panel.update(sample, nowMs + 41, 10);
  sample.backPressed = true;
  panel.update(sample, nowMs + 42, 10);
  TEST_ASSERT_EQUAL(OledControlPanelUpdate::Render,
                    panel.update(sample, nowMs + 52, 10));
  TEST_ASSERT_EQUAL_STRING("KIVO MENU", panel.frame(status).lines[0].c_str());

  sample.backPressed = false;
  panel.update(sample, nowMs + 53, 10);
  panel.update(sample, nowMs + 63, 10);
  sample.backPressed = true;
  panel.update(sample, nowMs + 64, 10);
  TEST_ASSERT_EQUAL(OledControlPanelUpdate::Dismiss,
                    panel.update(sample, nowMs + 74, 10));
  TEST_ASSERT_FALSE(panel.active());
}

void test_oled_control_panel_opens_on_encoder_rotation_when_closed() {
  OledControlPanel panel;
  OledControlPanelSample sample;
  std::uint32_t nowMs = 0;

  panel.update(sample, nowMs, 10);
  TEST_ASSERT_EQUAL(OledControlPanelUpdate::Render,
                    rotateOledEncoder(panel, sample, nowMs, true));
  TEST_ASSERT_TRUE(panel.active());
  TEST_ASSERT_EQUAL_STRING("  LIVE VIEW", panel.frame(DisplayFrame{}).lines[1].c_str());
  TEST_ASSERT_EQUAL_STRING("> SUB2API", panel.frame(DisplayFrame{}).lines[2].c_str());
}

void test_oled_control_panel_ignores_push_noise_during_encoder_rotation() {
  OledControlPanel panel;
  OledControlPanelSample sample;
  std::uint32_t nowMs = 0;

  panel.update(sample, nowMs, 10);
  sample.confirmPressed = true;
  panel.update(sample, ++nowMs, 10);
  nowMs += 10;
  TEST_ASSERT_EQUAL(OledControlPanelUpdate::Render,
                    panel.update(sample, nowMs, 10));
  sample.confirmPressed = false;
  panel.update(sample, ++nowMs, 10);
  nowMs += 10;
  panel.update(sample, nowMs, 10);

  // The encoder push line goes low at the same time as the A/B transitions.
  // It must not select LIVE VIEW and dismiss the menu.
  sample.encoderPressed = true;
  sample.encoderAHigh = false;
  panel.update(sample, ++nowMs, 10);
  sample.encoderBHigh = false;
  panel.update(sample, ++nowMs, 10);
  sample.encoderAHigh = true;
  panel.update(sample, ++nowMs, 10);
  sample.encoderBHigh = true;
  TEST_ASSERT_EQUAL(OledControlPanelUpdate::Render,
                    panel.update(sample, ++nowMs, 10));
  nowMs += 10;
  TEST_ASSERT_EQUAL(OledControlPanelUpdate::None,
                    panel.update(sample, nowMs, 10));
  TEST_ASSERT_TRUE(panel.active());
  TEST_ASSERT_EQUAL_STRING("> SUB2API",
                           panel.frame(DisplayFrame{}).lines[2].c_str());
}

OledControlPanelUpdate rotateOledEncoder(OledControlPanel &panel,
                                         OledControlPanelSample &sample,
                                         std::uint32_t &nowMs,
                                         bool clockwise) {
  if (clockwise) {
    sample.encoderAHigh = false;
    panel.update(sample, ++nowMs, 10);
    sample.encoderBHigh = false;
    panel.update(sample, ++nowMs, 10);
    sample.encoderAHigh = true;
    panel.update(sample, ++nowMs, 10);
    sample.encoderBHigh = true;
  } else {
    sample.encoderBHigh = false;
    panel.update(sample, ++nowMs, 10);
    sample.encoderAHigh = false;
    panel.update(sample, ++nowMs, 10);
    sample.encoderBHigh = true;
    panel.update(sample, ++nowMs, 10);
    sample.encoderAHigh = true;
  }
  return panel.update(sample, ++nowMs, 10);
}

void test_oled_control_panel_renders_cost_token_and_tpm_on_sub2api_page() {
  const OledUsageSnapshot usage{OledUsageState::Stale, 12345678ULL,
                                1234567ULL, 98765ULL};
  OledControlPanel panel;
  OledControlPanelSample sample;
  std::uint32_t nowMs = 0;
  panel.setUsageSnapshot(usage);
  panel.update(sample, nowMs, 10);
  sample.encoderPressed = true;
  panel.update(sample, ++nowMs, 10);
  nowMs += 10;
  panel.update(sample, nowMs, 10);
  sample.encoderPressed = false;
  panel.update(sample, ++nowMs, 10);
  nowMs += 10;
  panel.update(sample, nowMs, 10);
  rotateOledEncoder(panel, sample, nowMs, true);
  nowMs += 20;
  sample.confirmPressed = true;
  panel.update(sample, ++nowMs, 10);
  nowMs += 10;
  panel.update(sample, nowMs, 10);

  const auto frame = panel.frame(DisplayFrame{});
  TEST_ASSERT_EQUAL_STRING("SUB2API / STALE", frame.lines[0].c_str());
  TEST_ASSERT_EQUAL_STRING("COST $12.35", frame.lines[1].c_str());
  TEST_ASSERT_EQUAL_STRING("TOKENS 1M", frame.lines[2].c_str());
  TEST_ASSERT_EQUAL_STRING("TPM 98K", frame.lines[3].c_str());
}

void test_oled_control_panel_adjusts_brightness_with_the_encoder() {
  OledControlPanel panel;
  OledControlPanelSample sample;
  const DisplayFrame status{};
  std::uint32_t nowMs = 0;

  panel.update(sample, nowMs, 10);
  sample.encoderPressed = true;
  panel.update(sample, ++nowMs, 10);
  nowMs += 10;
  TEST_ASSERT_EQUAL(OledControlPanelUpdate::Render,
                    panel.update(sample, nowMs, 10));
  sample.encoderPressed = false;
  panel.update(sample, ++nowMs, 10);
  nowMs += 10;
  panel.update(sample, nowMs, 10);

  for (int step = 0; step < 4; ++step) {
    TEST_ASSERT_EQUAL(OledControlPanelUpdate::Render,
                      rotateOledEncoder(panel, sample, nowMs, true));
  }
  TEST_ASSERT_EQUAL_STRING("> BRIGHTNESS",
                           panel.frame(status).lines[3].c_str());

  nowMs += 20;
  sample.confirmPressed = true;
  panel.update(sample, ++nowMs, 10);
  nowMs += 10;
  TEST_ASSERT_EQUAL(OledControlPanelUpdate::Render,
                    panel.update(sample, nowMs, 10));
  TEST_ASSERT_EQUAL_UINT8(100, panel.brightnessPercent());
  TEST_ASSERT_EQUAL_STRING("DISPLAY BRIGHTNESS",
                           panel.frame(status).lines[0].c_str());
  TEST_ASSERT_EQUAL_STRING("LEVEL: 100%",
                           panel.frame(status).lines[1].c_str());
  TEST_ASSERT_EQUAL_STRING("[################]",
                           panel.frame(status).lines[2].c_str());

  sample.confirmPressed = false;
  panel.update(sample, ++nowMs, 10);
  nowMs += 10;
  panel.update(sample, nowMs, 10);
  TEST_ASSERT_EQUAL(OledControlPanelUpdate::BrightnessChanged,
                    rotateOledEncoder(panel, sample, nowMs, false));
  TEST_ASSERT_EQUAL_UINT8(95, panel.brightnessPercent());
  TEST_ASSERT_EQUAL_STRING("LEVEL: 95%",
                           panel.frame(status).lines[1].c_str());
  TEST_ASSERT_EQUAL_STRING("[###############.]",
                           panel.frame(status).lines[2].c_str());

  for (int step = 0; step < 18; ++step) {
    TEST_ASSERT_EQUAL(OledControlPanelUpdate::BrightnessChanged,
                      rotateOledEncoder(panel, sample, nowMs, false));
  }
  TEST_ASSERT_EQUAL_UINT8(5, panel.brightnessPercent());
  TEST_ASSERT_EQUAL(OledControlPanelUpdate::None,
                    rotateOledEncoder(panel, sample, nowMs, false));

  sample.backPressed = true;
  panel.update(sample, ++nowMs, 10);
  nowMs += 20;
  TEST_ASSERT_EQUAL(OledControlPanelUpdate::Render,
                    panel.update(sample, nowMs, 10));
  TEST_ASSERT_EQUAL_STRING("KIVO MENU", panel.frame(status).lines[0].c_str());

  panel.reset();
  TEST_ASSERT_EQUAL_UINT8(5, panel.brightnessPercent());
  TEST_ASSERT_FALSE(panel.active());
}

void test_oled_control_panel_clamps_loaded_brightness() {
  OledControlPanel panel;

  panel.setBrightnessPercent(55);
  TEST_ASSERT_EQUAL_UINT8(55, panel.brightnessPercent());
  panel.setBrightnessPercent(0);
  TEST_ASSERT_EQUAL_UINT8(5, panel.brightnessPercent());
  panel.setBrightnessPercent(255);
  TEST_ASSERT_EQUAL_UINT8(100, panel.brightnessPercent());
  panel.reset();
  TEST_ASSERT_EQUAL_UINT8(100, panel.brightnessPercent());
}

void commitFullScene(RemoteDisplay &display, std::uint32_t revision) {
  TEST_ASSERT_EQUAL(DisplayResult::Accepted,
                    display.begin(revision, 0, DisplayMode::Full));
  TEST_ASSERT_TRUE(display.region(0, {0, 0, 64, 16}));
  TEST_ASSERT_TRUE(display.clear(0));
  TEST_ASSERT_TRUE(display.text(0, 0, 12, 0, "BASE"));
  TEST_ASSERT_NOT_NULL(display.commit(revision));
}

void test_display_transaction_commits_atomically() {
  RemoteDisplay display;
  TEST_ASSERT_EQUAL(DisplayResult::Accepted,
                    display.begin(2, 0, DisplayMode::Full));
  TEST_ASSERT_TRUE(display.region(0, {0, 0, 64, 16}));
  TEST_ASSERT_TRUE(display.clear(0));
  TEST_ASSERT_TRUE(display.text(0, 0, 12, 0, "KIVO"));
  TEST_ASSERT_FALSE(display.committed().has_value());

  const auto commit = display.commit(2);

  TEST_ASSERT_NOT_NULL(commit);
  TEST_ASSERT_TRUE(display.lastCommit().has_value());
  TEST_ASSERT_EQUAL_PTR(&*display.lastCommit(), commit);
  TEST_ASSERT_EQUAL_UINT32(2, display.revision());
  TEST_ASSERT_TRUE(display.committed().has_value());
  TEST_ASSERT_EQUAL_UINT32(1, display.committed()->regionCount);
  TEST_ASSERT_EQUAL_UINT32(2, display.committed()->operationCount);
  TEST_ASSERT_EQUAL(DisplayOperationKind::Clear,
                    display.committed()->operations[0].kind);
  TEST_ASSERT_EQUAL(DisplayOperationKind::Text,
                    display.committed()->operations[1].kind);
  TEST_ASSERT_EQUAL_STRING(
      "KIVO", displayTextOperation(*display.committed(), 0)->text.c_str());
}

void test_display_revision_rules_request_resync_without_mutation() {
  RemoteDisplay display;
  commitFullScene(display, 4);

  TEST_ASSERT_EQUAL(DisplayResult::Resync,
                    display.begin(5, 3, DisplayMode::Delta));
  TEST_ASSERT_EQUAL_UINT32(4, display.revision());
  TEST_ASSERT_EQUAL_STRING(
      "BASE", displayTextOperation(*display.committed(), 0)->text.c_str());
  TEST_ASSERT_EQUAL(DisplayResult::Rejected,
                    display.begin(5, 4, DisplayMode::Full));
  TEST_ASSERT_EQUAL(DisplayResult::Rejected,
                    display.begin(5, 0, DisplayMode::Delta));
  TEST_ASSERT_EQUAL(DisplayResult::Rejected,
                    display.begin(0, 0, DisplayMode::Full));
  TEST_ASSERT_EQUAL(DisplayResult::Rejected,
                    display.begin(4, 4, DisplayMode::Delta));
}

void test_new_begin_discards_uncommitted_transaction() {
  RemoteDisplay display;
  TEST_ASSERT_EQUAL(DisplayResult::Accepted,
                    display.begin(1, 0, DisplayMode::Full));
  TEST_ASSERT_TRUE(display.region(0, {0, 0, 64, 16}));
  TEST_ASSERT_EQUAL(DisplayResult::Accepted,
                    display.begin(2, 0, DisplayMode::Full));
  TEST_ASSERT_NULL(display.commit(1));
}

void test_display_invalid_operation_discards_staging_without_mutation() {
  RemoteDisplay display;
  commitFullScene(display, 4);
  TEST_ASSERT_EQUAL(DisplayResult::Accepted,
                    display.begin(5, 4, DisplayMode::Delta));
  TEST_ASSERT_TRUE(display.region(0, {64, 0, 64, 16}));

  TEST_ASSERT_FALSE(display.text(0, 63, 12, 0, "OUTSIDE"));

  TEST_ASSERT_NULL(display.commit(5));
  TEST_ASSERT_EQUAL_UINT32(4, display.revision());
  TEST_ASSERT_EQUAL_STRING(
      "BASE", displayTextOperation(*display.committed(), 0)->text.c_str());
}

void test_display_regions_are_bounded_aligned_unique_and_fixed_capacity() {
  RemoteDisplay display;
  TEST_ASSERT_EQUAL(DisplayResult::Accepted,
                    display.begin(1, 0, DisplayMode::Full));
  TEST_ASSERT_FALSE(display.region(0, {0, 0, 0, 8}));
  TEST_ASSERT_NULL(display.commit(1));

  TEST_ASSERT_EQUAL(DisplayResult::Accepted,
                    display.begin(1, 0, DisplayMode::Full));
  TEST_ASSERT_FALSE(display.region(0, {4, 0, 8, 8}));
  TEST_ASSERT_EQUAL(DisplayResult::Accepted,
                    display.begin(1, 0, DisplayMode::Full));
  TEST_ASSERT_FALSE(display.region(0, {0, 4, 8, 8}));
  TEST_ASSERT_EQUAL(DisplayResult::Accepted,
                    display.begin(1, 0, DisplayMode::Full));
  TEST_ASSERT_FALSE(display.region(0, {120, 56, 16, 16}));

  TEST_ASSERT_EQUAL(DisplayResult::Accepted,
                    display.begin(1, 0, DisplayMode::Full));
  TEST_ASSERT_TRUE(display.region(0, {0, 0, 8, 8}));
  TEST_ASSERT_FALSE(display.region(0, {8, 0, 8, 8}));

  TEST_ASSERT_EQUAL(DisplayResult::Accepted,
                    display.begin(1, 0, DisplayMode::Full));
  for (std::uint8_t slot = 0; slot < kMaxDisplayRegions; ++slot) {
    TEST_ASSERT_TRUE(display.region(slot, {0, 0, 8, 8}));
  }
  TEST_ASSERT_FALSE(display.region(8, {0, 0, 8, 8}));
}

void test_display_operations_require_regions_and_enforce_total_limit() {
  RemoteDisplay display;
  TEST_ASSERT_EQUAL(DisplayResult::Accepted,
                    display.begin(1, 0, DisplayMode::Full));
  TEST_ASSERT_FALSE(display.clear(0));

  TEST_ASSERT_EQUAL(DisplayResult::Accepted,
                    display.begin(1, 0, DisplayMode::Full));
  TEST_ASSERT_TRUE(display.region(0, {0, 0, 128, 64}));
  for (std::size_t index = 0; index < kMaxDisplayOps; ++index) {
    TEST_ASSERT_TRUE(display.clear(0));
  }
  TEST_ASSERT_FALSE(display.clear(0));
  TEST_ASSERT_NULL(display.commit(1));
}

void test_display_text_accepts_declared_fonts_and_rejects_invalid_values() {
  RemoteDisplay display;
  const std::string longest(kMaxDisplayTextBytes, 'A');
  const std::string oversized(kMaxDisplayTextBytes + 1, 'A');
  const std::string control("BAD\nTEXT");
  const std::string nonAscii(1, static_cast<char>(0x80));

  TEST_ASSERT_EQUAL(DisplayResult::Accepted,
                    display.begin(1, 0, DisplayMode::Full));
  TEST_ASSERT_TRUE(display.region(0, {0, 0, 128, 64}));
  TEST_ASSERT_TRUE(display.text(0, 0, 0, 0, longest));
  TEST_ASSERT_FALSE(display.text(0, 0, 0, 0, oversized));

  for (std::uint8_t fontId = 0; fontId <= kRemoteDisplayMaxFontId;
       ++fontId) {
    RemoteDisplay supported;
    TEST_ASSERT_EQUAL(DisplayResult::Accepted,
                      supported.begin(1, 0, DisplayMode::Full));
    TEST_ASSERT_TRUE(supported.region(0, {0, 0, 128, 64}));
    TEST_ASSERT_TRUE(supported.text(0, 0, 21, fontId, "FONT"));
  }

  TEST_ASSERT_EQUAL(DisplayResult::Accepted,
                    display.begin(1, 0, DisplayMode::Full));
  TEST_ASSERT_TRUE(display.region(0, {0, 0, 128, 64}));
  TEST_ASSERT_FALSE(display.text(0, 0, 21, 3, "FONT"));
  TEST_ASSERT_EQUAL(DisplayResult::Accepted,
                    display.begin(1, 0, DisplayMode::Full));
  TEST_ASSERT_TRUE(display.region(0, {0, 0, 128, 64}));
  TEST_ASSERT_FALSE(display.text(0, 0, 0, 0, control));
  TEST_ASSERT_EQUAL(DisplayResult::Accepted,
                    display.begin(1, 0, DisplayMode::Full));
  TEST_ASSERT_TRUE(display.region(0, {0, 0, 128, 64}));
  TEST_ASSERT_FALSE(display.text(0, 0, 0, 0, nonAscii));
}

void test_display_delta_replaces_only_declared_slots_and_unions_dirty_bounds() {
  RemoteDisplay display;
  TEST_ASSERT_EQUAL(DisplayResult::Accepted,
                    display.begin(1, 0, DisplayMode::Full));
  TEST_ASSERT_TRUE(display.region(0, {0, 0, 64, 16}));
  TEST_ASSERT_TRUE(display.text(0, 0, 12, 0, "LEFT"));
  TEST_ASSERT_TRUE(display.region(1, {64, 0, 64, 16}));
  TEST_ASSERT_TRUE(display.text(1, 64, 12, 0, "RIGHT"));
  TEST_ASSERT_NOT_NULL(display.commit(1));

  TEST_ASSERT_EQUAL(DisplayResult::Accepted,
                    display.begin(2, 1, DisplayMode::Delta));
  TEST_ASSERT_TRUE(display.region(0, {0, 16, 64, 16}));
  TEST_ASSERT_TRUE(display.text(0, 0, 28, 0, "MOVED"));
  const auto commit = display.commit(2);

  TEST_ASSERT_NOT_NULL(commit);
  TEST_ASSERT_EQUAL_UINT32(2, commit->regionCount);
  TEST_ASSERT_EQUAL_UINT32(2, commit->operationCount);
  TEST_ASSERT_EQUAL_STRING("RIGHT", commit->operations[0].text.c_str());
  TEST_ASSERT_EQUAL_STRING("MOVED", commit->operations[1].text.c_str());
  TEST_ASSERT_EQUAL_STRING("MOVED",
                           displayTextOperation(*commit, 0)->text.c_str());
  TEST_ASSERT_EQUAL_STRING("RIGHT",
                           displayTextOperation(*commit, 1)->text.c_str());
  TEST_ASSERT_EQUAL_UINT32(1, commit->dirtyCount);
  TEST_ASSERT_EQUAL_UINT16(0, commit->dirtyBounds[0].x);
  TEST_ASSERT_EQUAL_UINT16(0, commit->dirtyBounds[0].y);
  TEST_ASSERT_EQUAL_UINT16(64, commit->dirtyBounds[0].width);
  TEST_ASSERT_EQUAL_UINT16(32, commit->dirtyBounds[0].height);
}

void test_display_full_commit_dirties_old_and_new_slot_union() {
  RemoteDisplay display;
  TEST_ASSERT_EQUAL(DisplayResult::Accepted,
                    display.begin(1, 0, DisplayMode::Full));
  TEST_ASSERT_TRUE(display.region(0, {0, 0, 64, 16}));
  auto first = display.commit(1);
  TEST_ASSERT_NOT_NULL(first);
  TEST_ASSERT_EQUAL_UINT32(1, first->dirtyCount);
  TEST_ASSERT_EQUAL_UINT16(64, first->dirtyBounds[0].width);
  TEST_ASSERT_EQUAL_UINT16(16, first->dirtyBounds[0].height);

  TEST_ASSERT_EQUAL(DisplayResult::Accepted,
                    display.begin(2, 0, DisplayMode::Full));
  TEST_ASSERT_TRUE(display.region(0, {0, 16, 64, 16}));
  const auto second = display.commit(2);

  TEST_ASSERT_NOT_NULL(second);
  TEST_ASSERT_EQUAL_UINT32(1, second->dirtyCount);
  TEST_ASSERT_EQUAL_UINT16(0, second->dirtyBounds[0].x);
  TEST_ASSERT_EQUAL_UINT16(0, second->dirtyBounds[0].y);
  TEST_ASSERT_EQUAL_UINT16(64, second->dirtyBounds[0].width);
  TEST_ASSERT_EQUAL_UINT16(32, second->dirtyBounds[0].height);
}

void test_display_layout_transitions_mark_the_full_panel_dirty() {
  RemoteDisplay display;
  TEST_ASSERT_EQUAL(DisplayResult::Accepted,
                    display.begin(1, 0, DisplayMode::Full));
  TEST_ASSERT_TRUE(display.region(0, {0, 0, 128, 64}));
  TEST_ASSERT_TRUE(display.clear(0));
  TEST_ASSERT_TRUE(display.text(0, 9, 38, 2, "CODEX 3 RUN"));
  TEST_ASSERT_NOT_NULL(display.commit(1));

  TEST_ASSERT_EQUAL(DisplayResult::Accepted,
                    display.begin(2, 1, DisplayMode::Delta));
  TEST_ASSERT_TRUE(display.region(0, {0, 0, 64, 32}));
  TEST_ASSERT_TRUE(display.clear(0));
  TEST_ASSERT_TRUE(display.text(0, 0, 23, 0, "CODEX"));
  TEST_ASSERT_TRUE(display.region(1, {64, 0, 64, 32}));
  TEST_ASSERT_TRUE(display.clear(1));
  TEST_ASSERT_TRUE(display.text(1, 64, 23, 0, "3 RUN"));
  TEST_ASSERT_TRUE(display.region(2, {0, 32, 128, 32}));
  TEST_ASSERT_TRUE(display.clear(2));
  const auto compact = display.commit(2);

  TEST_ASSERT_NOT_NULL(compact);
  TEST_ASSERT_TRUE(compact->dirtyCount >= 1);
  TEST_ASSERT_EQUAL_UINT16(0, compact->dirtyBounds[0].x);
  TEST_ASSERT_EQUAL_UINT16(0, compact->dirtyBounds[0].y);
  TEST_ASSERT_EQUAL_UINT16(128, compact->dirtyBounds[0].width);
  TEST_ASSERT_EQUAL_UINT16(64, compact->dirtyBounds[0].height);

  TEST_ASSERT_EQUAL(DisplayResult::Accepted,
                    display.begin(3, 0, DisplayMode::Full));
  TEST_ASSERT_TRUE(display.region(0, {0, 0, 128, 64}));
  TEST_ASSERT_TRUE(display.clear(0));
  TEST_ASSERT_TRUE(display.text(0, 9, 38, 2, "CODEX 3 RUN"));
  const auto full = display.commit(3);

  TEST_ASSERT_NOT_NULL(full);
  TEST_ASSERT_TRUE(full->dirtyCount >= 1);
  TEST_ASSERT_EQUAL_UINT16(0, full->dirtyBounds[0].x);
  TEST_ASSERT_EQUAL_UINT16(0, full->dirtyBounds[0].y);
  TEST_ASSERT_EQUAL_UINT16(128, full->dirtyBounds[0].width);
  TEST_ASSERT_EQUAL_UINT16(64, full->dirtyBounds[0].height);
}

void test_parses_display_commands_and_decodes_ascii_base64() {
  const auto begin = parseHelperCommand("DISPLAY_BEGIN 2 1 delta\n");
  TEST_ASSERT_TRUE(begin.has_value());
  TEST_ASSERT_EQUAL(HelperCommandKind::DisplayBegin, begin->kind);
  TEST_ASSERT_EQUAL_UINT32(2, begin->revision);
  TEST_ASSERT_EQUAL_UINT32(1, begin->baseRevision);
  TEST_ASSERT_FALSE(begin->displayFull);

  const auto full = parseHelperCommand("DISPLAY_BEGIN 1 0 full\n");
  TEST_ASSERT_TRUE(full.has_value());
  TEST_ASSERT_TRUE(full->displayFull);

  const auto region =
      parseHelperCommand("DISPLAY_REGION 1 64 0 64 16\n");
  TEST_ASSERT_TRUE(region.has_value());
  TEST_ASSERT_EQUAL(HelperCommandKind::DisplayRegion, region->kind);
  TEST_ASSERT_EQUAL_UINT8(1, region->displaySlot);
  TEST_ASSERT_EQUAL_UINT16(64, region->displayX);
  TEST_ASSERT_EQUAL_UINT16(0, region->displayY);
  TEST_ASSERT_EQUAL_UINT16(64, region->displayWidth);
  TEST_ASSERT_EQUAL_UINT16(16, region->displayHeight);

  const auto clear = parseHelperCommand("DISPLAY_CLEAR 1\n");
  TEST_ASSERT_TRUE(clear.has_value());
  TEST_ASSERT_EQUAL(HelperCommandKind::DisplayClear, clear->kind);
  TEST_ASSERT_EQUAL_UINT8(1, clear->displaySlot);

  const auto text = parseHelperCommand("DISPLAY_TEXT 1 64 12 0 S0lWTw==\n");
  TEST_ASSERT_TRUE(text.has_value());
  TEST_ASSERT_EQUAL(HelperCommandKind::DisplayText, text->kind);
  TEST_ASSERT_EQUAL_UINT8(1, text->displaySlot);
  TEST_ASSERT_EQUAL_UINT16(64, text->displayX);
  TEST_ASSERT_EQUAL_UINT16(12, text->displayY);
  TEST_ASSERT_EQUAL_UINT8(0, text->displayFontId);
  TEST_ASSERT_EQUAL_STRING("KIVO", text->displayText.c_str());

  const auto commit = parseHelperCommand("DISPLAY_COMMIT 2\n");
  TEST_ASSERT_TRUE(commit.has_value());
  TEST_ASSERT_EQUAL(HelperCommandKind::DisplayCommit, commit->kind);
  TEST_ASSERT_EQUAL_UINT32(2, commit->revision);
}

void test_rejects_malformed_display_command_tokens_numbers_and_base64() {
  const char *invalid[] = {
      "DISPLAY_BEGIN 2 1 Delta\n",
      "DISPLAY_BEGIN 2 1 delta trailing\n",
      "DISPLAY_BEGIN -2 0 full\n",
      "DISPLAY_REGION 1 0 0 64\n",
      "DISPLAY_REGION 256 0 0 64 16\n",
      "DISPLAY_REGION 1 65536 0 64 16\n",
      "DISPLAY_CLEAR 1 trailing\n",
      "DISPLAY_TEXT 1 0 12 0\n",
      "DISPLAY_TEXT 1 0 12 0 A===\n",
      "DISPLAY_TEXT 1 0 12 0 S0l*Tw==\n",
      "DISPLAY_TEXT 1 0 12 0 S0lWTw=\n",
      "DISPLAY_TEXT 1 0 12 0 S0lWTw==trailing\n",
      "DISPLAY_TEXT 1 0 12 0 QkFEClRFWFQ=\n",
      "DISPLAY_TEXT 1 0 12 0 AEFC\n",
      "DISPLAY_TEXT 1 0 12 0 gA==\n",
      "DISPLAY_COMMIT 2 trailing\n",
  };
  for (const auto line : invalid) {
    TEST_ASSERT_FALSE_MESSAGE(parseHelperCommand(line).has_value(), line);
  }

  const std::string oversizedDecoded(68, 'Q');
  TEST_ASSERT_FALSE(parseHelperCommand("DISPLAY_TEXT 1 0 12 0 " +
                                       oversizedDecoded + "\n")
                        .has_value());
  TEST_ASSERT_FALSE(parseHelperCommand(std::string(255, 'x')).has_value());
}

void test_display_dispatch_formats_exact_replies_and_clears_bad_staging() {
  RemoteDisplay display;
  const auto begin = *parseHelperCommand("DISPLAY_BEGIN 2 0 full\n");
  TEST_ASSERT_FALSE(dispatchDisplayCommand(display, begin, true).has_value());
  TEST_ASSERT_EQUAL_UINT32(2, display.stagedRevision().value_or(0));

  const auto malformed =
      discardMalformedDisplayCommand(display, "DISPLAY_TEXT 0 0 12 0 A===\n");
  TEST_ASSERT_TRUE(malformed.has_value());
  TEST_ASSERT_EQUAL_STRING("DISPLAY_ERROR 2 invalid_text\n",
                           malformed->c_str());
  TEST_ASSERT_FALSE(display.stagedRevision().has_value());
  TEST_ASSERT_FALSE(discardMalformedDisplayCommand(display, "UNKNOWN\n")
                        .has_value());
  TEST_ASSERT_EQUAL_STRING(
      "DISPLAY_ERROR 9 invalid_begin\n",
      discardMalformedDisplayCommand(display, "DISPLAY_BEGIN 9 nope full\n")
          ->c_str());
  TEST_ASSERT_EQUAL_STRING(
      "DISPLAY_ERROR 0 invalid_begin\n",
      discardMalformedDisplayCommand(display, "DISPLAY_BEGIN nope\n")
          ->c_str());
}

void test_exact_display_namespace_token_cancels_staging() {
  RemoteDisplay display;
  TEST_ASSERT_EQUAL(DisplayResult::Accepted,
                    display.begin(2, 0, DisplayMode::Full));

  const auto reply = discardMalformedDisplayCommand(display, "DISPLAY_\n");

  TEST_ASSERT_TRUE(reply.has_value());
  TEST_ASSERT_EQUAL_STRING("DISPLAY_ERROR 2 unsupported_display\n",
                           reply->c_str());
  TEST_ASSERT_FALSE(display.stagedRevision().has_value());
  TEST_ASSERT_NULL(display.commit(2));
}

void test_display_dispatch_replies_for_resync_commit_and_unsupported_panel() {
  RemoteDisplay display;
  commitFullScene(display, 4);
  const auto mismatch = *parseHelperCommand("DISPLAY_BEGIN 5 3 delta\n");
  TEST_ASSERT_EQUAL_STRING(
      "DISPLAY_RESYNC 4\n",
      dispatchDisplayCommand(display, mismatch, true)->c_str());

  const auto begin = *parseHelperCommand("DISPLAY_BEGIN 5 4 delta\n");
  TEST_ASSERT_FALSE(dispatchDisplayCommand(display, begin, true).has_value());
  const auto region = *parseHelperCommand("DISPLAY_REGION 0 0 0 64 16\n");
  TEST_ASSERT_FALSE(dispatchDisplayCommand(display, region, true).has_value());
  const auto text = *parseHelperCommand("DISPLAY_TEXT 0 0 12 0 TkVX\n");
  TEST_ASSERT_FALSE(dispatchDisplayCommand(display, text, true).has_value());
  const auto commit = *parseHelperCommand("DISPLAY_COMMIT 5\n");
  TEST_ASSERT_EQUAL_STRING("DISPLAY_OK 5\n",
                           dispatchDisplayCommand(display, commit, true)
                               ->c_str());

  RemoteDisplay unsupported;
  const auto unsupportedReply = dispatchDisplayCommand(
      unsupported, *parseHelperCommand("DISPLAY_BEGIN 1 0 full\n"), false);
  TEST_ASSERT_EQUAL_STRING("DISPLAY_ERROR 1 unsupported_display\n",
                           unsupportedReply->c_str());
}

void test_formats_display_protocol_replies_exactly() {
  TEST_ASSERT_EQUAL_STRING("DISPLAY_OK 7\n", formatDisplayOk(7).c_str());
  TEST_ASSERT_EQUAL_STRING("DISPLAY_RESYNC 4\n",
                           formatDisplayResync(4).c_str());
  TEST_ASSERT_EQUAL_STRING("DISPLAY_ERROR 9 invalid_commit\n",
                           formatDisplayError(9, "invalid_commit").c_str());
}

void test_commits_complete_matrix_topology_atomically() {
  TopologyBuilder builder(kYdEsp32S3);
  TEST_ASSERT_TRUE(builder.begin(7, 30));
  TEST_ASSERT_TRUE(builder.addMatrix(7, 0, {1, 2}, {12, 13}));

  const auto topology = builder.commit(7);

  TEST_ASSERT_TRUE(topology.has_value());
  TEST_ASSERT_EQUAL_UINT32(7, topology->revision);
  TEST_ASSERT_EQUAL_UINT8(2, topology->matrices[0].rows.size());
}

void test_runtime_topology_counts_direct_and_matrix_keys() {
  TopologyBuilder builder(kYdRp2040);
  TEST_ASSERT_TRUE(builder.begin(8, 30));
  TEST_ASSERT_TRUE(builder.addDirect(8, 0, {0, 1}));
  TEST_ASSERT_TRUE(builder.addMatrix(8, 1, {2, 3}, {4, 5}));

  const auto topology = builder.commit(8);

  TEST_ASSERT_TRUE(topology.has_value());
  TEST_ASSERT_EQUAL_UINT32(6, topology->keyCount());
}

void test_board_profiles_enforce_exact_safe_pins() {
  for (const auto pin : {0, 10, 11, 18, 21, 38, 39, 40, 41, 42, 47}) {
    TEST_ASSERT_TRUE(kYdEsp32S3.supports(pin));
  }
  TEST_ASSERT_FALSE(kYdEsp32S3.supports(19));
  TEST_ASSERT_FALSE(kYdEsp32S3.supports(20));
  TEST_ASSERT_FALSE(kYdEsp32S3.supports(35));
  TEST_ASSERT_FALSE(kYdEsp32S3.supports(36));
  TEST_ASSERT_FALSE(kYdEsp32S3.supports(37));
  TEST_ASSERT_FALSE(kYdEsp32S3.supports(43));
  TEST_ASSERT_FALSE(kYdEsp32S3.supports(44));
  TEST_ASSERT_FALSE(kYdEsp32S3.supports(45));
  TEST_ASSERT_FALSE(kYdEsp32S3.supports(46));
  TEST_ASSERT_FALSE(kYdEsp32S3.supports(48));
  TEST_ASSERT_TRUE(kYdRp2040.supports(0));
  TEST_ASSERT_TRUE(kYdRp2040.supports(22));
  TEST_ASSERT_TRUE(kYdRp2040.supports(23));
  for (std::uint8_t pin = 24; pin <= 25; ++pin) {
    TEST_ASSERT_FALSE(kYdRp2040.supports(pin));
  }
  for (std::uint8_t pin = 26; pin <= 29; ++pin) {
    TEST_ASSERT_TRUE(kYdRp2040.supports(pin));
  }

  TopologyBuilder rp2040(kYdRp2040);
  TEST_ASSERT_TRUE(rp2040.begin(1, 30));
  TEST_ASSERT_TRUE(rp2040.addDirect(1, 0, {0, 22, 23, 26, 27, 28, 29}));

  TopologyBuilder reserved(kYdRp2040);
  TEST_ASSERT_TRUE(reserved.begin(1, 30));
  TEST_ASSERT_FALSE(reserved.addDirect(1, 0, {24}));
}

void test_board_profiles_report_oled_capability() {
  TEST_ASSERT_FALSE(kYdEsp32S3.supportsOled);
  TEST_ASSERT_TRUE(kYdRp2040.supportsOled);
}

void test_rp2040_standalone_debug_topology_matches_keyboard_wiring() {
  const auto topology = makeRp2040StandaloneDebugTopology(kYdRp2040);

  TEST_ASSERT_TRUE(topology.has_value());
  TEST_ASSERT_EQUAL_UINT16(30, topology->debounceMs);
  TEST_ASSERT_EQUAL_UINT32(18, topology->keyCount());
  TEST_ASSERT_EQUAL_UINT32(1, topology->directs.size());
  TEST_ASSERT_EQUAL_UINT8(0, topology->directs[0].index);
  TEST_ASSERT_EQUAL_UINT32(18, topology->directs[0].pins.size());
  for (std::uint8_t pin = 1; pin <= 18; ++pin) {
    TEST_ASSERT_EQUAL_UINT8(pin, topology->directs[0].pins[pin - 1]);
  }
  TEST_ASSERT_TRUE(topology->matrices.empty());
  TEST_ASSERT_TRUE(topology->oled.has_value());
  TEST_ASSERT_EQUAL_UINT8(28, topology->oled->sda);
  TEST_ASSERT_EQUAL_UINT8(29, topology->oled->scl);

  TEST_ASSERT_FALSE(
      makeRp2040StandaloneDebugTopology(kYdEsp32S3).has_value());
}

void test_rp2040_oled_selects_hardware_i2c_when_pin_roles_match() {
  TEST_ASSERT_EQUAL(platform::Rp2040OledBus::I2c0,
                    platform::selectRp2040OledBus(28, 29));
  TEST_ASSERT_EQUAL(platform::Rp2040OledBus::I2c1,
                    platform::selectRp2040OledBus(26, 27));
  TEST_ASSERT_EQUAL(platform::Rp2040OledBus::I2c0,
                    platform::selectRp2040OledBus(4, 5));
}

void test_rp2040_oled_falls_back_to_software_i2c_for_arbitrary_safe_pins() {
  TEST_ASSERT_EQUAL(platform::Rp2040OledBus::Software,
                    platform::selectRp2040OledBus(28, 27));
  TEST_ASSERT_EQUAL(platform::Rp2040OledBus::Software,
                    platform::selectRp2040OledBus(29, 28));
  TEST_ASSERT_EQUAL(platform::Rp2040OledBus::Software,
                    platform::selectRp2040OledBus(5, 6));
}

void test_formats_protocol_v12_hello_with_board_and_build() {
  TEST_ASSERT_EQUAL_STRING(
      "HELLO 12 rp2040 yd-rp2040 0.1.0+gabc1234 - 28 "
      "0 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 19 20 21 22 23 26 "
      "27 28 29\n",
      formatHello(kYdRp2040, "0.1.0+gabc1234").c_str());
}

void test_formats_protocol_v12_hello_with_product_identity() {
  TEST_ASSERT_EQUAL_STRING(
      "HELLO 12 rp2040 yd-rp2040 0.1.0+gabc1234 key-k1-r01 28 "
      "0 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 19 20 21 22 23 26 "
      "27 28 29\n",
      formatHello(kYdRp2040, "0.1.0+gabc1234", "key-k1-r01")
          .c_str());
  TEST_ASSERT_EQUAL_STRING(
      "HELLO 12 rp2040 yd-rp2040 build - 28 "
      "0 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 19 20 21 22 23 26 "
      "27 28 29\n",
      formatHello(kYdRp2040, "build", "-").c_str());
}

void test_rejects_empty_firmware_build_id() {
  TEST_ASSERT_EQUAL_STRING("", formatHello(kYdRp2040, "").c_str());
}

void test_rejects_whitespace_in_firmware_build_id() {
  TEST_ASSERT_EQUAL_STRING("",
                           formatHello(kYdRp2040, "0.1.0 test").c_str());
}

void test_contact_edge_reports_unordered_pair_once_after_debounce() {
  TopologyBuilder builder(kYdEsp32S3);
  TEST_ASSERT_TRUE(builder.begin(7, 30));
  TEST_ASSERT_TRUE(builder.addMatrix(7, 0, {1, 2}, {12, 13}));
  GpioTriggerController controller(kYdEsp32S3, 0);
  controller.configure(*builder.commit(7), 0);

  TEST_ASSERT_FALSE(controller.updateContact(0, 1, 12, true, 10).has_value());
  const auto event = controller.updateContact(0, 12, 1, true, 40);

  TEST_ASSERT_TRUE(event.has_value());
  TEST_ASSERT_EQUAL_UINT8(1, event->input.pinA);
  TEST_ASSERT_EQUAL_UINT8(12, event->input.pinB);
  TEST_ASSERT_FALSE(controller.updateContact(0, 1, 12, true, 80).has_value());
}

void test_parses_runtime_configuration_commands() {
  const auto hello = parseHelperCommand("HELLO\n");
  TEST_ASSERT_TRUE(hello.has_value());
  TEST_ASSERT_EQUAL(HelperCommandKind::Hello, hello->kind);

  const auto productInfo = parseHelperCommand("PRODUCT_INFO\n");
  TEST_ASSERT_TRUE(productInfo.has_value());
  TEST_ASSERT_EQUAL(HelperCommandKind::ProductInfo, productInfo->kind);
  const auto productRead = parseHelperCommand("PRODUCT_READ\r\n");
  TEST_ASSERT_TRUE(productRead.has_value());
  TEST_ASSERT_EQUAL(HelperCommandKind::ProductRead, productRead->kind);
  TEST_ASSERT_FALSE(parseHelperCommand("PRODUCT_INFO extra\n").has_value());
  TEST_ASSERT_FALSE(parseHelperCommand("PRODUCT_READ 0\n").has_value());

  const auto begin = parseHelperCommand("CONFIG_BEGIN 3 30\n");
  TEST_ASSERT_TRUE(begin.has_value());
  TEST_ASSERT_EQUAL(HelperCommandKind::ConfigBegin, begin->kind);
  TEST_ASSERT_EQUAL_UINT32(3, begin->revision);
  TEST_ASSERT_EQUAL_UINT16(30, begin->debounceMs);

  const auto direct = parseHelperCommand("CONFIG_DIRECT 3 0 2 6 7\n");
  TEST_ASSERT_TRUE(direct.has_value());
  TEST_ASSERT_EQUAL(HelperCommandKind::ConfigDirect, direct->kind);
  TEST_ASSERT_EQUAL_UINT8(0, direct->sourceIndex);
  TEST_ASSERT_EQUAL_UINT8(2, direct->pins.size());

  const auto matrix =
      parseHelperCommand("CONFIG_MATRIX 3 1 2 1 2 2 12 13\n");
  TEST_ASSERT_TRUE(matrix.has_value());
  TEST_ASSERT_EQUAL(HelperCommandKind::ConfigMatrix, matrix->kind);
  TEST_ASSERT_EQUAL_UINT8(2, matrix->rows.size());
  TEST_ASSERT_EQUAL_UINT8(2, matrix->columns.size());

  const auto oled = parseHelperCommand("CONFIG_OLED 3 4 5\n");
  TEST_ASSERT_TRUE(oled.has_value());
  TEST_ASSERT_EQUAL(HelperCommandKind::ConfigOled, oled->kind);
  TEST_ASSERT_EQUAL_UINT32(3, oled->revision);
  TEST_ASSERT_EQUAL_UINT8(4, oled->oledSda);
  TEST_ASSERT_EQUAL_UINT8(5, oled->oledScl);

  const auto sh1106 = parseHelperCommand("CONFIG_SH1106 3 28 29\n");
  TEST_ASSERT_TRUE(sh1106.has_value());
  TEST_ASSERT_EQUAL(HelperCommandKind::ConfigSh1106, sh1106->kind);
  TEST_ASSERT_EQUAL_UINT8(28, sh1106->oledSda);
  TEST_ASSERT_EQUAL_UINT8(29, sh1106->oledScl);

  const auto oledControl =
      parseHelperCommand("CONFIG_OLED_CONTROL 3 19 20 21 22 26\n");
  TEST_ASSERT_TRUE(oledControl.has_value());
  TEST_ASSERT_EQUAL(HelperCommandKind::ConfigOledControl,
                    oledControl->kind);
  TEST_ASSERT_EQUAL_UINT32(3, oledControl->revision);
  TEST_ASSERT_EQUAL_UINT8(5, oledControl->pins.size());
  TEST_ASSERT_EQUAL_UINT8(19, oledControl->pins[0]);
  TEST_ASSERT_EQUAL_UINT8(26, oledControl->pins[4]);

  const auto commit = parseHelperCommand("CONFIG_COMMIT 3\n");
  TEST_ASSERT_TRUE(commit.has_value());
  TEST_ASSERT_EQUAL(HelperCommandKind::ConfigCommit, commit->kind);

  const auto usage = parseHelperCommand(
      "USAGE 3 18446744073709551615 1234567890123 987654321\n");
  TEST_ASSERT_TRUE(usage.has_value());
  TEST_ASSERT_EQUAL(HelperCommandKind::Usage, usage->kind);
  TEST_ASSERT_EQUAL_UINT8(3, usage->usageState);
  TEST_ASSERT_EQUAL_UINT64(UINT64_MAX, usage->usageCostMicros);
  TEST_ASSERT_EQUAL_UINT64(1234567890123ULL, usage->usageTodayTokens);
  TEST_ASSERT_EQUAL_UINT64(987654321ULL, usage->usageTpm);
  TEST_ASSERT_FALSE(parseHelperCommand("USAGE 8 1 2 3\n").has_value());
  TEST_ASSERT_FALSE(parseHelperCommand(
                        "USAGE 2 18446744073709551616 2 3\n")
                        .has_value());
}

void test_oled_configuration_requires_supported_distinct_safe_pins() {
  TopologyBuilder esp32(kYdEsp32S3);
  TEST_ASSERT_TRUE(esp32.begin(1, 30));
  TEST_ASSERT_FALSE(esp32.addOled(1, 4, 5));

  TopologyBuilder rp2040(kYdRp2040);
  TEST_ASSERT_TRUE(rp2040.begin(1, 30));
  TEST_ASSERT_FALSE(rp2040.addOled(1, 4, 4));
  TEST_ASSERT_FALSE(rp2040.addOled(1, 4, 24));
  TEST_ASSERT_TRUE(rp2040.addOled(1, 28, 29));
  const auto topology = rp2040.commit(1);
  TEST_ASSERT_TRUE(topology.has_value());
  TEST_ASSERT_TRUE(topology->oled.has_value());
  TEST_ASSERT_EQUAL_UINT8(28, topology->oled->sda);
  TEST_ASSERT_EQUAL_UINT8(29, topology->oled->scl);
  TEST_ASSERT_EQUAL(OledDriver::Ssd1306, topology->oled->driver);

  TopologyBuilder sh1106(kYdRp2040);
  TEST_ASSERT_TRUE(sh1106.begin(2, 30));
  TEST_ASSERT_TRUE(sh1106.addSh1106(2, 28, 29));
  const auto sh1106Topology = sh1106.commit(2);
  TEST_ASSERT_TRUE(sh1106Topology.has_value());
  TEST_ASSERT_EQUAL(OledDriver::Sh1106, sh1106Topology->oled->driver);
}

void test_oled_pins_are_reserved_when_oled_command_arrives_first() {
  TopologyBuilder builder(kYdRp2040);
  TEST_ASSERT_TRUE(builder.begin(1, 30));
  TEST_ASSERT_TRUE(builder.addOled(1, 4, 5));
  TEST_ASSERT_FALSE(builder.addDirect(1, 0, {6, 5}));
  TEST_ASSERT_TRUE(builder.addDirect(1, 0, {6}));

  const auto topology = builder.commit(1);
  TEST_ASSERT_TRUE(topology.has_value());
  TEST_ASSERT_EQUAL_UINT8(1, topology->directs[0].pins.size());
  TEST_ASSERT_EQUAL_UINT8(6, topology->directs[0].pins[0]);
}

void test_oled_pins_are_reserved_when_input_command_arrives_first() {
  TopologyBuilder builder(kYdRp2040);
  TEST_ASSERT_TRUE(builder.begin(1, 30));
  TEST_ASSERT_TRUE(builder.addDirect(1, 0, {4}));
  TEST_ASSERT_FALSE(builder.addOled(1, 5, 4));
  TEST_ASSERT_TRUE(builder.addOled(1, 5, 6));

  const auto topology = builder.commit(1);
  TEST_ASSERT_TRUE(topology.has_value());
  TEST_ASSERT_TRUE(topology->oled.has_value());
  TEST_ASSERT_EQUAL_UINT8(5, topology->oled->sda);
  TEST_ASSERT_EQUAL_UINT8(6, topology->oled->scl);
}

void test_oled_control_panel_reserves_five_pins_without_counting_keys() {
  TopologyBuilder builder(kYdRp2040);
  TEST_ASSERT_TRUE(builder.begin(1, 30));
  TEST_ASSERT_FALSE(builder.addOledControlPanel(1, 19, 20, 21, 22, 26));
  TEST_ASSERT_TRUE(builder.addOled(1, 28, 29));
  TEST_ASSERT_FALSE(builder.addOledControlPanel(1, 19, 19, 21, 22, 26));
  TEST_ASSERT_TRUE(builder.addOledControlPanel(1, 19, 20, 21, 22, 26));
  TEST_ASSERT_FALSE(builder.addDirect(1, 0, {6, 19}));
  TEST_ASSERT_TRUE(builder.addDirect(1, 0, {6}));

  const auto topology = builder.commit(1);
  TEST_ASSERT_TRUE(topology.has_value());
  TEST_ASSERT_TRUE(topology->oledControlPanel.has_value());
  TEST_ASSERT_EQUAL_UINT8(19, topology->oledControlPanel->confirm);
  TEST_ASSERT_EQUAL_UINT8(26, topology->oledControlPanel->back);
  TEST_ASSERT_EQUAL_UINT8(1, topology->keyCount());
}

void test_parses_extended_registered_board_pin_domain() {
  const auto direct = parseHelperCommand(
      "CONFIG_DIRECT 3 0 9 10 11 19 20 22 26 27 28 29\n");

  TEST_ASSERT_TRUE(direct.has_value());
  TEST_ASSERT_EQUAL_UINT8(9, direct->pins.size());
  TEST_ASSERT_EQUAL_UINT8(10, direct->pins[0]);
  TEST_ASSERT_EQUAL_UINT8(29, direct->pins[8]);

  TopologyBuilder esp32(kYdEsp32S3);
  TEST_ASSERT_TRUE(esp32.begin(3, 30));
  TEST_ASSERT_FALSE(esp32.addDirect(3, 0, direct->pins));

  TopologyBuilder rp2040(kYdRp2040);
  TEST_ASSERT_TRUE(rp2040.begin(3, 30));
  TEST_ASSERT_TRUE(rp2040.addDirect(3, 0, direct->pins));
}

void test_parser_defers_unsupported_pins_to_board_validation() {
  const auto direct = parseHelperCommand("CONFIG_DIRECT 3 0 3 23 24 25\n");

  TEST_ASSERT_TRUE(direct.has_value());
  TEST_ASSERT_EQUAL_UINT8(3, direct->pins.size());
  TEST_ASSERT_EQUAL_UINT8(23, direct->pins[0]);
  TEST_ASSERT_EQUAL_UINT8(25, direct->pins[2]);

  TopologyBuilder esp32(kYdEsp32S3);
  TEST_ASSERT_TRUE(esp32.begin(3, 30));
  TEST_ASSERT_FALSE(esp32.addDirect(3, 0, direct->pins));

  TopologyBuilder rp2040(kYdRp2040);
  TEST_ASSERT_TRUE(rp2040.begin(3, 30));
  TEST_ASSERT_FALSE(rp2040.addDirect(3, 0, direct->pins));
}

void test_parses_learning_and_ordered_action_commands() {
  const auto begin = parseHelperCommand("LEARN_BEGIN 4 4 1 2 12 13\n");
  TEST_ASSERT_TRUE(begin.has_value());
  TEST_ASSERT_EQUAL(HelperCommandKind::LearnBegin, begin->kind);
  TEST_ASSERT_EQUAL_UINT8(4, begin->pins.size());

  const auto end = parseHelperCommand("LEARN_END 4\n");
  TEST_ASSERT_TRUE(end.has_value());
  TEST_ASSERT_EQUAL(HelperCommandKind::LearnEnd, end->kind);

  const auto paste = parseHelperCommand("PASTE 9 1 2\n");
  TEST_ASSERT_TRUE(paste.has_value());
  TEST_ASSERT_EQUAL(HelperCommandKind::Paste, paste->kind);
  TEST_ASSERT_EQUAL_UINT16(1, paste->step);
  TEST_ASSERT_EQUAL_UINT16(2, paste->total);

  const auto hotkey = parseHelperCommand("HOTKEY 9 2 2 0 40\n");
  TEST_ASSERT_TRUE(hotkey.has_value());
  TEST_ASSERT_EQUAL(HelperCommandKind::Hotkey, hotkey->kind);
  TEST_ASSERT_EQUAL_UINT8(40, hotkey->keycode);

  const auto delay = parseHelperCommand("DELAY 10 1 4 60000\n");
  TEST_ASSERT_TRUE(delay.has_value());
  TEST_ASSERT_EQUAL(HelperCommandKind::Delay, delay->kind);
  TEST_ASSERT_EQUAL_UINT32(60000, delay->durationMs);

  const auto media = parseHelperCommand("MEDIA 10 2 4 205\n");
  TEST_ASSERT_TRUE(media.has_value());
  TEST_ASSERT_EQUAL(HelperCommandKind::Media, media->kind);
  TEST_ASSERT_EQUAL_UINT16(205, media->consumerUsage);

  const auto host = parseHelperCommand("HOST 10 3 4\n");
  TEST_ASSERT_TRUE(host.has_value());
  TEST_ASSERT_EQUAL(HelperCommandKind::Host, host->kind);

  const auto skip = parseHelperCommand("SKIP 9\n");
  TEST_ASSERT_TRUE(skip.has_value());
  TEST_ASSERT_EQUAL(HelperCommandKind::Skip, skip->kind);
}

void test_rejects_malformed_runtime_commands() {
  TEST_ASSERT_FALSE(
      parseHelperCommand("CONFIG_DIRECT 3 0 2 6\n").has_value());
  TEST_ASSERT_FALSE(parseHelperCommand(
                        "CONFIG_MATRIX 3 1 2 1 1 2 12 13\n")
                        .has_value());
  TEST_ASSERT_FALSE(
      parseHelperCommand("LEARN_BEGIN 4 2 1 1\n").has_value());
  TEST_ASSERT_FALSE(parseHelperCommand("CONFIG_OLED 3 4\n").has_value());
  TEST_ASSERT_FALSE(
      parseHelperCommand("CONFIG_OLED 3 4 256\n").has_value());
  TEST_ASSERT_FALSE(
      parseHelperCommand("CONFIG_OLED 3 4 5 trailing\n").has_value());
  TEST_ASSERT_FALSE(parseHelperCommand("CONFIG_SH1106 3 4\n").has_value());
  TEST_ASSERT_FALSE(
      parseHelperCommand("CONFIG_SH1106 3 4 256\n").has_value());
  TEST_ASSERT_FALSE(
      parseHelperCommand("CONFIG_SH1106 3 4 5 trailing\n").has_value());
  TEST_ASSERT_FALSE(
      parseHelperCommand("CONFIG_OLED_CONTROL 3 19 20 21 22\n")
          .has_value());
  TEST_ASSERT_FALSE(
      parseHelperCommand("CONFIG_OLED_CONTROL 3 19 20 21 22 256\n")
          .has_value());
  TEST_ASSERT_FALSE(
      parseHelperCommand("CONFIG_OLED_CONTROL 3 19 20 21 22 26 trailing\n")
          .has_value());
  TEST_ASSERT_FALSE(parseHelperCommand("PASTE 9 0 2\n").has_value());
  TEST_ASSERT_FALSE(parseHelperCommand("HOTKEY 9 2 1 0 40\n").has_value());
  TEST_ASSERT_FALSE(parseHelperCommand("DELAY 9 1 1 0\n").has_value());
  TEST_ASSERT_FALSE(parseHelperCommand("DELAY 9 1 1 60001\n").has_value());
  TEST_ASSERT_FALSE(parseHelperCommand("MEDIA 9 1 1 1\n").has_value());
  TEST_ASSERT_FALSE(parseHelperCommand("HOST 9 1 1 trailing\n").has_value());
  TEST_ASSERT_FALSE(parseHelperCommand(std::string(256, 'x')).has_value());
}

void test_debounced_input_edges_do_not_create_action_state() {
  auto controller = directController(0);
  ActionRunController runs;
  controller.updatePin(6, false, 0);
  const auto first = controller.updatePin(6, false, 30);
  TEST_ASSERT_TRUE(first.has_value());
  TEST_ASSERT_EQUAL_UINT32(1, first->id);
  TEST_ASSERT_FALSE(runs.hasActiveRun());

  controller.updatePin(6, true, 40);
  const auto released = controller.updatePin(6, true, 70);
  TEST_ASSERT_TRUE(released.has_value());
  TEST_ASSERT_EQUAL_UINT32(2, released->id);
  controller.updatePin(6, false, 80);
  const auto second = controller.updatePin(6, false, 110);
  TEST_ASSERT_TRUE(second.has_value());
  TEST_ASSERT_EQUAL_UINT32(3, second->id);
  TEST_ASSERT_FALSE(runs.hasActiveRun());
}

void test_learning_reports_contact_and_restores_runtime_topology() {
  TopologyBuilder builder(kYdEsp32S3);
  builder.begin(7, 30);
  builder.addDirect(7, 0, {6});
  GpioTriggerController controller(kYdEsp32S3, 0);
  controller.configure(*builder.commit(7), 0);

  TEST_ASSERT_FALSE(controller.beginLearning(4, {1, 12, 35}, 0));
  TEST_ASSERT_TRUE(controller.beginLearning(4, {1, 12}, 0));
  TEST_ASSERT_FALSE(
      controller.updateLearningContact(12, 1, true, 10).has_value());
  const auto event = controller.updateLearningContact(1, 12, true, 40);

  TEST_ASSERT_TRUE(event.has_value());
  TEST_ASSERT_EQUAL_STRING("LEARN_CONTACT 1 12 DOWN\n",
                           formatLearningEvent(*event).c_str());
  TEST_ASSERT_FALSE(controller.endLearning(5, 50));
  TEST_ASSERT_TRUE(controller.endLearning(4, 50));
  TEST_ASSERT_EQUAL_UINT32(7, controller.topology().revision);
}

void test_rp2040_learning_accepts_gpio23_and_gpio29() {
  GpioTriggerController esp32(kYdEsp32S3, 0);
  TEST_ASSERT_FALSE(esp32.beginLearning(4, {29}, 0));

  GpioTriggerController rp2040(kYdRp2040, 0);
  TEST_ASSERT_TRUE(rp2040.beginLearning(4, {29}, 0));
  TEST_ASSERT_TRUE(rp2040.endLearning(4, 1));
  TEST_ASSERT_TRUE(rp2040.beginLearning(5, {23}, 1));
  TEST_ASSERT_TRUE(rp2040.endLearning(5, 2));
  TEST_ASSERT_FALSE(rp2040.beginLearning(6, {24}, 2));
}

void test_learning_rejects_active_oled_pins() {
  TopologyBuilder builder(kYdRp2040);
  TEST_ASSERT_TRUE(builder.begin(7, 30));
  TEST_ASSERT_TRUE(builder.addDirect(7, 0, {6}));
  TEST_ASSERT_TRUE(builder.addOled(7, 4, 5));
  TEST_ASSERT_TRUE(builder.addOledControlPanel(7, 19, 20, 21, 22, 26));
  GpioTriggerController controller(kYdRp2040, 0);
  controller.configure(*builder.commit(7), 0);

  TEST_ASSERT_FALSE(controller.beginLearning(8, {4, 7}, 0));
  TEST_ASSERT_FALSE(controller.beginLearning(8, {5, 7}, 0));
  TEST_ASSERT_FALSE(controller.beginLearning(8, {19, 7}, 0));
  TEST_ASSERT_FALSE(controller.beginLearning(8, {22, 7}, 0));
  TEST_ASSERT_TRUE(controller.beginLearning(8, {7, 8}, 0));
}

void test_display_status_frames_have_two_sixteen_character_status_lines() {
  DisplayStatusModel status;

  auto frame = status.frame();
  TEST_ASSERT_EQUAL_STRING("KIVO     USB OFF", frame.lines[0].c_str());
  TEST_ASSERT_EQUAL_STRING("WAITING CONFIG  ", frame.lines[1].c_str());
  TEST_ASSERT_EQUAL_STRING("", frame.lines[2].c_str());

  status.setUsbConnected(true);
  status.setReady(18);
  frame = status.frame();
  TEST_ASSERT_EQUAL_STRING("KIVO      USB ON", frame.lines[0].c_str());
  TEST_ASSERT_EQUAL_STRING("READY    18 KEYS", frame.lines[1].c_str());

  status.setLearning(18);
  TEST_ASSERT_EQUAL_STRING("LEARNING 18 PINS",
                           status.frame().lines[1].c_str());
  status.setConfigError();
  TEST_ASSERT_EQUAL_STRING("CONFIG ERROR    ",
                           status.frame().lines[1].c_str());

  for (std::size_t index = 0; index < 2; ++index) {
    TEST_ASSERT_EQUAL_UINT32(16, status.frame().lines[index].size());
  }
}

void test_standalone_debug_display_does_not_depend_on_usb_state() {
  DisplayStatusModel status;
  status.setStandaloneDebug(true);
  status.setReady(18);

  TEST_ASSERT_EQUAL_STRING("KIVO GPIO DEBUG ",
                           status.frame().lines[0].c_str());
  status.setUsbConnected(true);
  TEST_ASSERT_EQUAL_STRING("KIVO GPIO DEBUG ",
                           status.frame().lines[0].c_str());
  TEST_ASSERT_EQUAL_STRING("READY    18 KEYS",
                           status.frame().lines[1].c_str());
}

void test_standalone_debug_display_returns_to_managed_usb_status() {
  DisplayStatusModel status;
  status.setStandaloneDebug(true);
  status.setUsbConnected(true);
  status.setReady(18);

  status.setStandaloneDebug(false);

  TEST_ASSERT_EQUAL_STRING("KIVO      USB ON",
                           status.frame().lines[0].c_str());
}

void test_display_status_formats_last_direct_and_contact_edges() {
  DisplayStatusModel status;

  status.recordInput(InputEvent{1, 12, InputState::Down});
  TEST_ASSERT_EQUAL_STRING("12 D", status.frame().lines[2].c_str());
  status.recordInput(InputEvent{2, 12, InputState::Up});
  TEST_ASSERT_EQUAL_STRING("12 U", status.frame().lines[2].c_str());
  status.recordInput(InputEvent{3, 5, InputState::Down});
  TEST_ASSERT_EQUAL_STRING("5 D", status.frame().lines[2].c_str());
  status.recordInput(InputEvent{3, PhysicalInput::contact(0, 12, 1),
                                InputState::Down});
  TEST_ASSERT_EQUAL_STRING("1-12 D", status.frame().lines[2].c_str());
  status.recordInput(InputEvent{4, PhysicalInput::contact(0, 1, 12),
                                InputState::Up});
  TEST_ASSERT_EQUAL_STRING("1-12 U", status.frame().lines[2].c_str());
}

void test_display_status_can_clear_stale_input_activity() {
  DisplayStatusModel status;
  status.recordInput(InputEvent{1, 5, InputState::Down});

  status.clearLastInput();

  TEST_ASSERT_EQUAL_STRING("", status.frame().lines[2].c_str());
}

void test_suppresses_new_contact_that_closes_a_ghost_cycle() {
  TopologyBuilder builder(kYdEsp32S3);
  builder.begin(1, 1);
  builder.addMatrix(1, 0, {1, 2}, {12, 13});
  GpioTriggerController controller(kYdEsp32S3, 0);
  controller.configure(*builder.commit(1), 0);

  for (const auto pair :
       {std::pair<std::uint8_t, std::uint8_t>{1, 12}, {1, 13}, {2, 12}}) {
    controller.updateContact(0, pair.first, pair.second, true, 0);
    TEST_ASSERT_TRUE(
        controller.updateContact(0, pair.first, pair.second, true, 1)
            .has_value());
  }
  controller.updateContact(0, 2, 13, true, 2);
  TEST_ASSERT_FALSE(controller.updateContact(0, 2, 13, true, 3).has_value());
}

void test_exposes_supported_gpio_inputs() {
  GpioTriggerController controller(kYdEsp32S3);
  TEST_ASSERT_TRUE(controller.isSupportedPin(0));
  TEST_ASSERT_TRUE(controller.isSupportedPin(1));
  TEST_ASSERT_TRUE(controller.isSupportedPin(9));
  TEST_ASSERT_TRUE(controller.isSupportedPin(10));
  TEST_ASSERT_TRUE(controller.isSupportedPin(11));
  TEST_ASSERT_TRUE(controller.isSupportedPin(12));
  TEST_ASSERT_TRUE(controller.isSupportedPin(18));
  TEST_ASSERT_TRUE(controller.isSupportedPin(21));
  TEST_ASSERT_TRUE(controller.isSupportedPin(38));
  TEST_ASSERT_TRUE(controller.isSupportedPin(47));
  TEST_ASSERT_FALSE(controller.isSupportedPin(19));
  TEST_ASSERT_FALSE(controller.isSupportedPin(35));
  TEST_ASSERT_FALSE(controller.isSupportedPin(48));
}

void test_stable_edges_emit_once_after_debounce() {
  auto controller = directController(0);

  TEST_ASSERT_FALSE(controller.updatePin(6, false, 1000).has_value());
  TEST_ASSERT_FALSE(controller.updatePin(6, true, 1010).has_value());
  TEST_ASSERT_FALSE(controller.updatePin(6, false, 1020).has_value());
  TEST_ASSERT_FALSE(controller.updatePin(6, false, 1049).has_value());

  const auto down = controller.updatePin(6, false, 1050);
  TEST_ASSERT_TRUE(down.has_value());
  TEST_ASSERT_EQUAL_UINT32(1, down->id);
  TEST_ASSERT_EQUAL_UINT8(6, down->gpio);
  TEST_ASSERT_EQUAL(InputState::Down, down->state);
  TEST_ASSERT_FALSE(controller.updatePin(6, false, 1100).has_value());

  TEST_ASSERT_FALSE(controller.updatePin(6, true, 1200).has_value());
  TEST_ASSERT_FALSE(controller.updatePin(6, false, 1210).has_value());
  TEST_ASSERT_FALSE(controller.updatePin(6, true, 1220).has_value());
  TEST_ASSERT_FALSE(controller.updatePin(6, true, 1249).has_value());

  const auto up = controller.updatePin(6, true, 1250);
  TEST_ASSERT_TRUE(up.has_value());
  TEST_ASSERT_EQUAL_UINT32(2, up->id);
  TEST_ASSERT_EQUAL_UINT8(6, up->gpio);
  TEST_ASSERT_EQUAL(InputState::Up, up->state);
}

void test_key_activity_indicator_recolors_each_press_and_clears_on_final_release() {
  KeyActivityIndicator indicator;

  TEST_ASSERT_EQUAL(KeyIndicatorAction::ShowRandomColor,
                    indicator.handle(InputState::Down));
  TEST_ASSERT_EQUAL(KeyIndicatorAction::ShowRandomColor,
                    indicator.handle(InputState::Down));
  TEST_ASSERT_EQUAL(KeyIndicatorAction::None,
                    indicator.handle(InputState::Up));
  TEST_ASSERT_EQUAL(KeyIndicatorAction::Off,
                    indicator.handle(InputState::Up));
}

void test_key_activity_indicator_reset_discards_held_state() {
  KeyActivityIndicator indicator;
  TEST_ASSERT_EQUAL(KeyIndicatorAction::ShowRandomColor,
                    indicator.handle(InputState::Down));

  indicator.reset();

  TEST_ASSERT_EQUAL(KeyIndicatorAction::None,
                    indicator.handle(InputState::Up));
  TEST_ASSERT_EQUAL(KeyIndicatorAction::ShowRandomColor,
                    indicator.handle(InputState::Down));
  TEST_ASSERT_EQUAL(KeyIndicatorAction::Off,
                    indicator.handle(InputState::Up));
}

void test_release_rearms_pin_for_a_later_press() {
  auto controller = directController(0);

  controller.updatePin(6, false, 0);
  const auto first = controller.updatePin(6, false, 30);
  TEST_ASSERT_TRUE(first.has_value());

  controller.updatePin(6, true, 40);
  controller.updatePin(6, true, 70);
  controller.updatePin(6, false, 80);
  const auto second = controller.updatePin(6, false, 110);

  TEST_ASSERT_TRUE(second.has_value());
  TEST_ASSERT_EQUAL_UINT32(3, second->id);
}

void test_debounce_survives_millisecond_clock_rollover() {
  constexpr std::uint32_t kBeforeRollover = 0xFFFFFFF5U;
  auto controller = directController(kBeforeRollover);

  TEST_ASSERT_FALSE(
      controller.updatePin(6, false, kBeforeRollover).has_value());
  TEST_ASSERT_FALSE(controller.updatePin(6, false, 0x00000012U).has_value());

  const auto event = controller.updatePin(6, false, 0x00000013U);
  TEST_ASSERT_TRUE(event.has_value());
  TEST_ASSERT_EQUAL_UINT8(6, event->gpio);
}

void test_serializes_input_state_events() {
  TEST_ASSERT_EQUAL_STRING(
      "STATE 42 DIRECT 6 DOWN\n",
      formatInputEvent(InputEvent{42, 6, InputState::Down}).c_str());
  TEST_ASSERT_EQUAL_STRING(
      "STATE 43 CONTACT 1 6 12 UP\n",
      formatInputEvent(InputEvent{43, PhysicalInput::contact(1, 12, 6),
                                  InputState::Up})
          .c_str());
  TEST_ASSERT_EQUAL_STRING(
      "DONE 43 2\n",
      formatDone(43, 2).c_str());
}

void test_parses_paste_and_skip_responses() {
  const auto paste = parseHelperCommand("PASTE 42 1 1\n");
  TEST_ASSERT_TRUE(paste.has_value());
  TEST_ASSERT_EQUAL(HelperCommandKind::Paste, paste->kind);
  TEST_ASSERT_EQUAL_UINT32(42, paste->runId);

  const auto skip = parseHelperCommand("SKIP 7\r\n");
  TEST_ASSERT_TRUE(skip.has_value());
  TEST_ASSERT_EQUAL(HelperCommandKind::Skip, skip->kind);
  TEST_ASSERT_EQUAL_UINT32(7, skip->runId);
}

void test_rejects_malformed_responses() {
  TEST_ASSERT_FALSE(parseHelperCommand("PASTE\n").has_value());
  TEST_ASSERT_FALSE(parseHelperCommand("PASTE nope 1 1\n").has_value());
  TEST_ASSERT_FALSE(parseHelperCommand("PASTE 1 1 1 trailing\n").has_value());
  TEST_ASSERT_FALSE(parseHelperCommand("OTHER 1\n").has_value());
}

void test_parses_hotkey_response() {
  const auto response = parseHelperCommand("HOTKEY 42 1 1 10 14\n");
  TEST_ASSERT_TRUE(response.has_value());
  TEST_ASSERT_EQUAL(HelperCommandKind::Hotkey, response->kind);
  TEST_ASSERT_EQUAL_UINT32(42, response->runId);
  TEST_ASSERT_EQUAL_UINT8(10, response->modifierMask);
  TEST_ASSERT_EQUAL_UINT8(14, response->keycode);
}

void test_rejects_malformed_hotkey_response() {
  TEST_ASSERT_FALSE(parseHelperCommand("HOTKEY 42 1 1 10\n").has_value());
  TEST_ASSERT_FALSE(parseHelperCommand("HOTKEY 42 1 1 256 14\n").has_value());
  TEST_ASSERT_FALSE(parseHelperCommand("HOTKEY 42 1 1 10 0\n").has_value());
  TEST_ASSERT_FALSE(parseHelperCommand("HOTKEY 42 1 1 10 165\n").has_value());
}

void test_accepts_at_most_254_byte_protocol_lines() {
  const std::string command = "PASTE 1 1 1\n";
  const auto at_limit = std::string(254 - command.size(), ' ') + command;
  TEST_ASSERT_EQUAL_UINT32(254, at_limit.size());
  TEST_ASSERT_TRUE(parseHelperCommand(at_limit).has_value());

  const auto over_limit = std::string(255 - command.size(), ' ') + command;
  TEST_ASSERT_EQUAL_UINT32(255, over_limit.size());
  TEST_ASSERT_FALSE(parseHelperCommand(over_limit).has_value());
}

void test_parses_v6_chord_and_rejects_malformed_chords() {
  const auto chord = parseHelperCommand("CHORD 7 1 2 128 2 4 5\n");
  TEST_ASSERT_TRUE(chord.has_value());
  TEST_ASSERT_EQUAL(HelperCommandKind::Chord, chord->kind);
  TEST_ASSERT_EQUAL_UINT8(2, chord->keycodes.size());
  TEST_ASSERT_EQUAL_UINT8(5, chord->keycodes[1]);
  TEST_ASSERT_TRUE(parseHelperCommand("CHORD 8 1 1 128 0\n").has_value());
  TEST_ASSERT_FALSE(parseHelperCommand("CHORD 8 1 1 0 0\n").has_value());
  TEST_ASSERT_FALSE(parseHelperCommand("CHORD 8 1 1 0 2 4 4\n").has_value());
  TEST_ASSERT_FALSE(parseHelperCommand("CHORD 8 1 1 0 1 0\n").has_value());
  TEST_ASSERT_FALSE(parseHelperCommand("CHORD 8 1 1 0 7 4 5 6 7 8 9 10\n")
                        .has_value());
  TEST_ASSERT_FALSE(parseHelperCommand("CHORD 8 1 1 1 2 4\n").has_value());
}

void test_v6_run_starts_on_step_one_and_is_independent_of_input_ids() {
  ActionRunController runs;
  TEST_ASSERT_EQUAL(ResponseAction::Ignored, runs.acceptStep(41, 2, 2, 0));
  TEST_ASSERT_EQUAL(ResponseAction::Execute, runs.acceptStep(41, 1, 2, 1));
  TEST_ASSERT_EQUAL(ResponseAction::Execute, runs.acceptStep(41, 2, 2, 2));
  TEST_ASSERT_FALSE(runs.hasActiveRun());
}

void test_v6_run_cancel_keepalive_and_expiry() {
  ActionRunController runs;
  TEST_ASSERT_EQUAL(ResponseAction::Execute, runs.acceptStep(42, 1, 2, 1));
  TEST_ASSERT_TRUE(runs.keepAlive(42, 2000));
  runs.expire(3999);
  TEST_ASSERT_TRUE(runs.hasActiveRun());
  runs.expire(4000);
  TEST_ASSERT_FALSE(runs.hasActiveRun());

  TEST_ASSERT_EQUAL(ResponseAction::Execute, runs.acceptStep(43, 1, 2, 4001));
  TEST_ASSERT_EQUAL(ResponseAction::Cleared, runs.cancel(43));
  TEST_ASSERT_FALSE(runs.hasActiveRun());
  TEST_ASSERT_EQUAL(ResponseAction::Ignored, runs.cancel(43));
}

void test_v6_run_reset_clears_active_run() {
  ActionRunController runs;
  TEST_ASSERT_EQUAL(ResponseAction::Execute, runs.acceptStep(42, 1, 2, 1));

  runs.reset();

  TEST_ASSERT_FALSE(runs.hasActiveRun());
  TEST_ASSERT_EQUAL(ResponseAction::Ignored, runs.acceptStep(42, 2, 2, 2));
}

void test_v6_run_keepalive_rejects_an_expired_intermediate_step() {
  ActionRunController runs;
  TEST_ASSERT_EQUAL(ResponseAction::Execute, runs.acceptStep(42, 1, 2, 1));

  TEST_ASSERT_FALSE(
      runs.keepAlive(42, 1 + ActionRunController::kResponseTimeoutMs));
  TEST_ASSERT_FALSE(runs.hasActiveRun());
}

void test_keyboard_chord_dispatches_six_keys_then_emits_done() {
  const auto chord = parseHelperCommand("CHORD 61 1 1 255 6 4 5 6 7 8 9\n");
  TEST_ASSERT_TRUE(chord.has_value());
  ActionRunController runs;
  std::uint8_t sentModifiers = 0;
  std::array<std::uint8_t, 6> sentKeys{};
  std::vector<std::pair<std::uint32_t, std::uint16_t>> completions;

  const bool executed = executeKeyboardChord(
      runs, *chord, 100,
      [&](std::uint8_t modifiers, const std::array<std::uint8_t, 6> &keys) {
        sentModifiers = modifiers;
        sentKeys = keys;
        return true;
      },
      [&](std::uint32_t runId, std::uint16_t step) {
        completions.emplace_back(runId, step);
      });

  TEST_ASSERT_TRUE(executed);
  TEST_ASSERT_EQUAL_UINT8(0xFF, sentModifiers);
  TEST_ASSERT_EQUAL_UINT8(4, sentKeys[0]);
  TEST_ASSERT_EQUAL_UINT8(9, sentKeys[5]);
  TEST_ASSERT_EQUAL_UINT32(1, completions.size());
  TEST_ASSERT_EQUAL_UINT32(61, completions[0].first);
  TEST_ASSERT_EQUAL_UINT16(1, completions[0].second);
}

void test_keyboard_chord_failure_prevents_done() {
  const auto chord = parseHelperCommand("CHORD 62 1 1 128 0\n");
  TEST_ASSERT_TRUE(chord.has_value());
  ActionRunController runs;
  std::size_t completionCount = 0;

  const bool executed = executeKeyboardChord(
      runs, *chord, 100,
      [](std::uint8_t, const std::array<std::uint8_t, 6> &) { return false; },
      [&](std::uint32_t, std::uint16_t) { ++completionCount; });

  TEST_ASSERT_FALSE(executed);
  TEST_ASSERT_EQUAL_UINT32(0, completionCount);
}

void test_keyboard_chord_uses_action_run_ordering() {
  const auto first = parseHelperCommand("CHORD 63 1 2 0 1 4\n");
  const auto second = parseHelperCommand("CHORD 63 2 2 0 1 5\n");
  TEST_ASSERT_TRUE(first.has_value());
  TEST_ASSERT_TRUE(second.has_value());
  ActionRunController runs;
  std::size_t sendCount = 0;
  std::size_t completionCount = 0;
  const auto send = [&](std::uint8_t,
                        const std::array<std::uint8_t, 6> &) {
    ++sendCount;
    return true;
  };
  const auto complete = [&](std::uint32_t, std::uint16_t) {
    ++completionCount;
  };

  TEST_ASSERT_FALSE(executeKeyboardChord(runs, *second, 100, send, complete));
  TEST_ASSERT_TRUE(executeKeyboardChord(runs, *first, 101, send, complete));
  TEST_ASSERT_TRUE(executeKeyboardChord(runs, *second, 102, send, complete));
  TEST_ASSERT_EQUAL_UINT32(2, sendCount);
  TEST_ASSERT_EQUAL_UINT32(2, completionCount);
}

void test_active_delay_run_keeps_scanning_inputs_and_accepts_next_step() {
  ActionRunController runs;
  auto inputs = directController(0);
  TEST_ASSERT_EQUAL(ResponseAction::Execute, runs.acceptStep(64, 1, 2, 1));
  TEST_ASSERT_TRUE(runs.keepAlive(64, 2000));

  inputs.updatePin(6, false, 2001);
  const auto input = inputs.updatePin(6, false, 2031);

  TEST_ASSERT_TRUE(input.has_value());
  TEST_ASSERT_EQUAL(ResponseAction::Execute, runs.acceptStep(64, 2, 2, 3999));
}

void test_discards_the_rest_of_an_overlong_physical_line() {
  ResponseLineBuffer lines(16);

  std::optional<ResponseLineEvent> overflow;
  for (const char character : std::string("OVERLONG RESPONSE PASTE 42 1 1\n")) {
    const auto next = lines.push(character);
    if (next.has_value()) overflow = next;
  }
  TEST_ASSERT_TRUE(overflow.has_value());
  TEST_ASSERT_TRUE(overflow->overflow);
  TEST_ASSERT_EQUAL_UINT32(16, overflow->line.size());

  std::optional<ResponseLineEvent> response;
  for (const char character : std::string("PASTE 7 1 1\n")) {
    response = lines.push(character);
  }
  TEST_ASSERT_TRUE(response.has_value());
  TEST_ASSERT_FALSE(response->overflow);
  TEST_ASSERT_EQUAL_STRING("PASTE 7 1 1\n", response->line.c_str());
}

void test_overlong_display_line_surfaces_prefix_and_cancels_staging() {
  ResponseLineBuffer lines(32);
  RemoteDisplay display;
  TEST_ASSERT_EQUAL(DisplayResult::Accepted,
                    display.begin(7, 0, DisplayMode::Full));

  std::optional<ResponseLineEvent> event;
  const std::string line =
      "DISPLAY_TEXT 0 0 12 0 " + std::string(64, 'A') + "\n";
  for (const char character : line) {
    const auto next = lines.push(character);
    if (next.has_value()) event = next;
  }

  TEST_ASSERT_TRUE(event.has_value());
  TEST_ASSERT_TRUE(event->overflow);
  TEST_ASSERT_EQUAL_UINT32(32, event->line.size());
  const auto reply = discardMalformedDisplayCommand(display, event->line);
  TEST_ASSERT_TRUE(reply.has_value());
  TEST_ASSERT_EQUAL_STRING("DISPLAY_ERROR 7 invalid_text\n",
                           reply->c_str());
  TEST_ASSERT_FALSE(display.stagedRevision().has_value());
  TEST_ASSERT_NULL(display.commit(7));
}

void test_overlong_unknown_line_stays_silent_and_preserves_display_staging() {
  ResponseLineBuffer lines(16);
  RemoteDisplay display;
  TEST_ASSERT_EQUAL(DisplayResult::Accepted,
                    display.begin(8, 0, DisplayMode::Full));

  std::optional<ResponseLineEvent> event;
  for (const char character : std::string("UNKNOWN COMMAND THAT IS TOO LONG\n")) {
    const auto next = lines.push(character);
    if (next.has_value()) event = next;
  }

  TEST_ASSERT_TRUE(event.has_value());
  TEST_ASSERT_TRUE(event->overflow);
  TEST_ASSERT_FALSE(
      discardMalformedDisplayCommand(display, event->line).has_value());
  TEST_ASSERT_EQUAL_UINT32(8, display.stagedRevision().value_or(0));
}

void test_hid_hotkey_waits_for_press_and_release_report_slots() {
  const bool readiness[] = {false, false, true, false, true};
  std::size_t readinessIndex = 0;
  std::vector<std::pair<std::uint8_t, std::uint8_t>> reports;
  std::size_t pauses = 0;

  const bool sent = platform::transmitHotkeyReports(
      0x08, 0x28, 4,
      [&]() { return readiness[readinessIndex++]; },
      [&](std::uint8_t modifiers, std::uint8_t keycode) {
        reports.emplace_back(modifiers, keycode);
        return true;
      },
      [&]() { ++pauses; });

  TEST_ASSERT_TRUE(sent);
  TEST_ASSERT_EQUAL_UINT32(5, readinessIndex);
  TEST_ASSERT_EQUAL_UINT32(3, pauses);
  TEST_ASSERT_EQUAL_UINT32(2, reports.size());
  TEST_ASSERT_EQUAL_UINT8(0x08, reports[0].first);
  TEST_ASSERT_EQUAL_UINT8(0x28, reports[0].second);
  TEST_ASSERT_EQUAL_UINT8(0, reports[1].first);
  TEST_ASSERT_EQUAL_UINT8(0, reports[1].second);
}

void test_keyboard_chord_sends_pressed_then_empty_release_report() {
  const auto reports = captureKeyboardReports(0xFF, {4, 5, 6, 7, 8, 9});

  TEST_ASSERT_EQUAL_UINT32(2, reports.size());
  TEST_ASSERT_EQUAL_UINT8(0xFF, reports[0].modifiers);
  TEST_ASSERT_EQUAL_UINT8(4, reports[0].keys[0]);
  TEST_ASSERT_EQUAL_UINT8(9, reports[0].keys[5]);
  TEST_ASSERT_EQUAL_UINT8(0, reports[1].modifiers);
  TEST_ASSERT_EQUAL_UINT8(0, reports[1].keys[0]);
  TEST_ASSERT_EQUAL_UINT8(0, reports[1].keys[5]);
}

void test_modifier_only_chord_is_not_dropped() {
  const auto reports = captureKeyboardReports(0x80, {});

  TEST_ASSERT_EQUAL_UINT32(2, reports.size());
  TEST_ASSERT_EQUAL_UINT8(0x80, reports[0].modifiers);
  TEST_ASSERT_EQUAL_UINT8(0, reports[0].keys[0]);
  TEST_ASSERT_EQUAL_UINT8(0, reports[1].modifiers);
}

void test_keyboard_chord_preserves_every_modifier_bit() {
  constexpr std::array<std::uint8_t, 8> modifierBits = {
      0x01, 0x02, 0x04, 0x08, 0x10, 0x20, 0x40, 0x80};

  for (const auto modifier : modifierBits) {
    const auto reports = captureKeyboardReports(modifier, {4, 5, 6, 7, 8, 9});
    TEST_ASSERT_EQUAL_UINT8(modifier, reports[0].modifiers);
  }
}

void test_keyboard_chord_send_failure_returns_false_without_release() {
  std::size_t sentReports = 0;
  const bool sent = platform::transmitKeyboardReports(
      0x08, {4, 5, 6, 7, 8, 9}, 0, []() { return true; },
      [&sentReports](const platform::KeyboardReport &) {
        ++sentReports;
        return false;
      },
      []() {});

  TEST_ASSERT_FALSE(sent);
  TEST_ASSERT_EQUAL_UINT32(1, sentReports);
}

void test_hid_consumer_control_waits_for_press_and_release_report_slots() {
  const bool readiness[] = {false, true, false, true};
  std::size_t readinessIndex = 0;
  std::vector<std::uint16_t> reports;
  std::size_t pauses = 0;

  const bool sent = platform::transmitConsumerReports(
      0x00CD, 3, [&]() { return readiness[readinessIndex++]; },
      [&](std::uint16_t usage) {
        reports.push_back(usage);
        return true;
      },
      [&]() { ++pauses; });

  TEST_ASSERT_TRUE(sent);
  TEST_ASSERT_EQUAL_UINT32(4, readinessIndex);
  TEST_ASSERT_EQUAL_UINT32(2, pauses);
  TEST_ASSERT_EQUAL_UINT32(2, reports.size());
  TEST_ASSERT_EQUAL_UINT16(0x00CD, reports[0]);
  TEST_ASSERT_EQUAL_UINT16(0, reports[1]);
}

int main(int, char **) {
  UNITY_BEGIN();
  RUN_TEST(test_dirty_tiles_emit_only_changed_counter_region);
  RUN_TEST(test_dirty_tiles_respect_per_loop_budget_and_coalesce_updates);
  RUN_TEST(test_dirty_tiles_round_outward_clip_and_stay_within_one_row);
  RUN_TEST(test_dirty_tiles_reject_sub_tile_budget_and_clear_explicitly);
  RUN_TEST(test_rotated_or_unsupported_panel_requests_full_refresh);
  RUN_TEST(test_startup_stays_local_until_first_full_remote_scene);
  RUN_TEST(test_startup_refresh_does_not_demote_remote_or_critical_content);
  RUN_TEST(test_display_reconfiguration_redraws_the_current_visible_source);
  RUN_TEST(test_local_critical_overrides_and_then_restores_latest_remote_scene);
  RUN_TEST(test_learning_override_retains_remote_and_restores_on_runtime_return);
  RUN_TEST(test_normal_input_debug_does_not_overwrite_active_remote_scene);
  RUN_TEST(test_disconnect_discards_remote_and_reconnect_requires_new_full_scene);
  RUN_TEST(test_reconnect_preserves_the_critical_override_from_before_disconnect);
  RUN_TEST(test_disconnected_remote_commit_is_discarded_before_reconnect_full);
  RUN_TEST(test_delayed_display_reconfiguration_preserves_offline_after_disconnect);
  RUN_TEST(test_interactive_panel_restores_the_latest_remote_scene);
  RUN_TEST(test_interactive_panel_returns_to_offline_status_without_remote_content);
  RUN_TEST(test_oled_control_panel_navigates_status_and_back_to_live_view);
  RUN_TEST(test_oled_control_panel_opens_on_encoder_rotation_when_closed);
  RUN_TEST(test_oled_control_panel_ignores_push_noise_during_encoder_rotation);
  RUN_TEST(test_oled_control_panel_renders_cost_token_and_tpm_on_sub2api_page);
  RUN_TEST(test_oled_control_panel_adjusts_brightness_with_the_encoder);
  RUN_TEST(test_oled_control_panel_clamps_loaded_brightness);
  RUN_TEST(test_display_transaction_commits_atomically);
  RUN_TEST(test_display_revision_rules_request_resync_without_mutation);
  RUN_TEST(test_new_begin_discards_uncommitted_transaction);
  RUN_TEST(test_display_invalid_operation_discards_staging_without_mutation);
  RUN_TEST(test_display_regions_are_bounded_aligned_unique_and_fixed_capacity);
  RUN_TEST(test_display_operations_require_regions_and_enforce_total_limit);
  RUN_TEST(test_display_text_accepts_declared_fonts_and_rejects_invalid_values);
  RUN_TEST(test_display_delta_replaces_only_declared_slots_and_unions_dirty_bounds);
  RUN_TEST(test_display_full_commit_dirties_old_and_new_slot_union);
  RUN_TEST(test_display_layout_transitions_mark_the_full_panel_dirty);
  RUN_TEST(test_parses_display_commands_and_decodes_ascii_base64);
  RUN_TEST(test_rejects_malformed_display_command_tokens_numbers_and_base64);
  RUN_TEST(test_display_dispatch_formats_exact_replies_and_clears_bad_staging);
  RUN_TEST(test_exact_display_namespace_token_cancels_staging);
  RUN_TEST(test_display_dispatch_replies_for_resync_commit_and_unsupported_panel);
  RUN_TEST(test_formats_display_protocol_replies_exactly);
  RUN_TEST(test_commits_complete_matrix_topology_atomically);
  RUN_TEST(test_runtime_topology_counts_direct_and_matrix_keys);
  RUN_TEST(test_board_profiles_enforce_exact_safe_pins);
  RUN_TEST(test_board_profiles_report_oled_capability);
  RUN_TEST(test_rp2040_standalone_debug_topology_matches_keyboard_wiring);
  RUN_TEST(test_rp2040_oled_selects_hardware_i2c_when_pin_roles_match);
  RUN_TEST(test_rp2040_oled_falls_back_to_software_i2c_for_arbitrary_safe_pins);
  RUN_TEST(test_formats_protocol_v12_hello_with_board_and_build);
  RUN_TEST(test_formats_protocol_v12_hello_with_product_identity);
  RUN_TEST(test_rejects_empty_firmware_build_id);
  RUN_TEST(test_rejects_whitespace_in_firmware_build_id);
  RUN_TEST(test_contact_edge_reports_unordered_pair_once_after_debounce);
  RUN_TEST(test_parses_runtime_configuration_commands);
  RUN_TEST(test_oled_configuration_requires_supported_distinct_safe_pins);
  RUN_TEST(test_oled_pins_are_reserved_when_oled_command_arrives_first);
  RUN_TEST(test_oled_pins_are_reserved_when_input_command_arrives_first);
  RUN_TEST(test_oled_control_panel_reserves_five_pins_without_counting_keys);
  RUN_TEST(test_parses_extended_registered_board_pin_domain);
  RUN_TEST(test_parser_defers_unsupported_pins_to_board_validation);
  RUN_TEST(test_parses_learning_and_ordered_action_commands);
  RUN_TEST(test_rejects_malformed_runtime_commands);
  RUN_TEST(test_debounced_input_edges_do_not_create_action_state);
  RUN_TEST(test_learning_reports_contact_and_restores_runtime_topology);
  RUN_TEST(test_rp2040_learning_accepts_gpio23_and_gpio29);
  RUN_TEST(test_learning_rejects_active_oled_pins);
  RUN_TEST(test_display_status_frames_have_two_sixteen_character_status_lines);
  RUN_TEST(test_standalone_debug_display_does_not_depend_on_usb_state);
  RUN_TEST(test_standalone_debug_display_returns_to_managed_usb_status);
  RUN_TEST(test_display_status_formats_last_direct_and_contact_edges);
  RUN_TEST(test_display_status_can_clear_stale_input_activity);
  RUN_TEST(test_suppresses_new_contact_that_closes_a_ghost_cycle);
  RUN_TEST(test_exposes_supported_gpio_inputs);
  RUN_TEST(test_stable_edges_emit_once_after_debounce);
  RUN_TEST(test_key_activity_indicator_recolors_each_press_and_clears_on_final_release);
  RUN_TEST(test_key_activity_indicator_reset_discards_held_state);
  RUN_TEST(test_release_rearms_pin_for_a_later_press);
  RUN_TEST(test_debounce_survives_millisecond_clock_rollover);
  RUN_TEST(test_serializes_input_state_events);
  RUN_TEST(test_parses_paste_and_skip_responses);
  RUN_TEST(test_rejects_malformed_responses);
  RUN_TEST(test_parses_hotkey_response);
  RUN_TEST(test_rejects_malformed_hotkey_response);
  RUN_TEST(test_accepts_at_most_254_byte_protocol_lines);
  RUN_TEST(test_parses_v6_chord_and_rejects_malformed_chords);
  RUN_TEST(test_v6_run_starts_on_step_one_and_is_independent_of_input_ids);
  RUN_TEST(test_v6_run_cancel_keepalive_and_expiry);
  RUN_TEST(test_v6_run_reset_clears_active_run);
  RUN_TEST(test_v6_run_keepalive_rejects_an_expired_intermediate_step);
  RUN_TEST(test_keyboard_chord_dispatches_six_keys_then_emits_done);
  RUN_TEST(test_keyboard_chord_failure_prevents_done);
  RUN_TEST(test_keyboard_chord_uses_action_run_ordering);
  RUN_TEST(test_active_delay_run_keeps_scanning_inputs_and_accepts_next_step);
  RUN_TEST(test_discards_the_rest_of_an_overlong_physical_line);
  RUN_TEST(test_overlong_display_line_surfaces_prefix_and_cancels_staging);
  RUN_TEST(test_overlong_unknown_line_stays_silent_and_preserves_display_staging);
  RUN_TEST(test_hid_hotkey_waits_for_press_and_release_report_slots);
  RUN_TEST(test_keyboard_chord_sends_pressed_then_empty_release_report);
  RUN_TEST(test_modifier_only_chord_is_not_dropped);
  RUN_TEST(test_keyboard_chord_preserves_every_modifier_bit);
  RUN_TEST(test_keyboard_chord_send_failure_returns_false_without_release);
  RUN_TEST(test_hid_consumer_control_waits_for_press_and_release_report_slots);
  return UNITY_END();
}
