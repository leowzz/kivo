#include <unity.h>

#include "DisplayRuntime.h"
#include "ControlPanel.h"

void setUp() {}
void tearDown() {}

namespace {
class FakeSurface : public DisplaySurface {
 public:
  bool configureOk = true;
  bool renderOk = true;
  bool transferOk = true;
  unsigned remoteFrames = 0;
  bool lastFull = false;
  DisplayFrame local{};
  bool configure(const std::optional<DisplayConfig> &) override { return configureOk; }
  bool renderLocal(const DisplayFrame &frame) override { local = frame; return true; }
  bool renderRemote(const RemoteDisplayCommit &, bool full) override {
    ++remoteFrames;
    lastFull = full;
    return renderOk;
  }
  void reset() override {}
  void setBrightness(std::uint8_t) override {}
  bool service() override { return transferOk; }
};

std::optional<std::string> send(DisplayRuntime &runtime, const char *line) {
  const auto command = parseHelperCommand(line);
  TEST_ASSERT_TRUE(command.has_value());
  return runtime.handle(*command);
}

void configure(DisplayRuntime &runtime, DisplayPanel panel = DisplayPanel::Ssd1306_128x32) {
  TEST_ASSERT_TRUE(runtime.configure(DisplayConfig{panel, 28, 29}, { {"KIVO", "READY", "", ""} }, 100));
  runtime.connectionChanged(true, { {"KIVO", "READY", "", ""} });
}

void stage(DisplayRuntime &runtime, std::uint32_t revision = 1) {
  const auto begin = "DISPLAY_BEGIN " + std::to_string(revision) + " 0 full\n";
  send(runtime, begin.c_str());
  send(runtime, "DISPLAY_REGION 0 0 0 128 32\n");
  send(runtime, "DISPLAY_TEXT 0 0 12 0 S0lWTw==\n");
}

void test_actual_panel_geometry_rejects_before_commit() {
  FakeSurface surface;
  DisplayRuntime runtime(surface);
  configure(runtime);
  send(runtime, "DISPLAY_BEGIN 1 0 full\n");
  const auto error = send(runtime, "DISPLAY_REGION 0 0 0 128 64\n");
  TEST_ASSERT_EQUAL_STRING("DISPLAY_ERROR 1 invalid_region\n", error->c_str());
  TEST_ASSERT_EQUAL_UINT(0, surface.remoteFrames);
  stage(runtime);
  TEST_ASSERT_EQUAL_STRING("DISPLAY_OK 1\n", send(runtime, "DISPLAY_COMMIT 1\n")->c_str());
}

void test_render_failure_never_acknowledges_success() {
  FakeSurface surface;
  DisplayRuntime runtime(surface);
  configure(runtime);
  surface.renderOk = false;
  stage(runtime);
  TEST_ASSERT_EQUAL_STRING("DISPLAY_ERROR 1 render_failed\n",
                          send(runtime, "DISPLAY_COMMIT 1\n")->c_str());
  TEST_ASSERT_EQUAL_STRING("DISPLAY ERROR", surface.local.lines[1].c_str());
}

void test_menu_preserves_remote_scene_and_restores_full_frame() {
  FakeSurface surface;
  DisplayRuntime runtime(surface);
  configure(runtime);
  runtime.showInteractive({ {"MENU", "BRIGHTNESS", "", ""} });
  stage(runtime);
  TEST_ASSERT_EQUAL_STRING("DISPLAY_OK 1\n", send(runtime, "DISPLAY_COMMIT 1\n")->c_str());
  TEST_ASSERT_EQUAL_UINT(0, surface.remoteFrames);
  runtime.dismissInteractive();
  TEST_ASSERT_EQUAL_UINT(1, surface.remoteFrames);
  TEST_ASSERT_TRUE(surface.lastFull);
}

void test_panel_change_discards_old_scene_and_revision() {
  FakeSurface surface;
  DisplayRuntime runtime(surface);
  configure(runtime, DisplayPanel::Sh1106_128x64);
  stage(runtime);
  send(runtime, "DISPLAY_COMMIT 1\n");
  configure(runtime);
  send(runtime, "DISPLAY_BEGIN 2 1 delta\n");
  TEST_ASSERT_EQUAL_STRING("DISPLAY_ERROR 2 invalid_commit\n",
                          send(runtime, "DISPLAY_COMMIT 2\n")->c_str());
  stage(runtime);
  TEST_ASSERT_EQUAL_STRING("DISPLAY_OK 1\n", send(runtime, "DISPLAY_COMMIT 1\n")->c_str());
}

void test_transfer_failure_is_reported_once_and_disables_acceptance() {
  FakeSurface surface;
  DisplayRuntime runtime(surface);
  configure(runtime);
  stage(runtime);
  send(runtime, "DISPLAY_COMMIT 1\n");
  surface.transferOk = false;
  TEST_ASSERT_EQUAL_STRING("DISPLAY_ERROR 1 transfer_failed\n", runtime.service()->c_str());
  TEST_ASSERT_FALSE(runtime.service().has_value());
  TEST_ASSERT_EQUAL_STRING("DISPLAY_ERROR 2 unsupported_display\n",
                          send(runtime, "DISPLAY_BEGIN 2 1 delta\n")->c_str());
}

void test_unconfigured_or_failed_display_rejects_remote_commands() {
  FakeSurface surface;
  DisplayRuntime runtime(surface);
  TEST_ASSERT_EQUAL_STRING("DISPLAY_ERROR 1 unsupported_display\n",
                          send(runtime, "DISPLAY_BEGIN 1 0 full\n")->c_str());
  surface.configureOk = false;
  TEST_ASSERT_FALSE(runtime.configure(DisplayConfig{}, { {"KIVO", "READY", "", ""} }, 100));
  TEST_ASSERT_EQUAL_STRING("DISPLAY_ERROR 1 unsupported_display\n",
                          send(runtime, "DISPLAY_BEGIN 1 0 full\n")->c_str());
}

void test_compact_menu_keeps_selection_visible_and_uses_actual_panel_info() {
  ControlPanel panel;
  ControlPanelSample sample;
  panel.update(sample, 0, 5);
  sample.confirmPressed = true;
  panel.update(sample, 1, 5);
  panel.update(sample, 6, 5);
  sample.confirmPressed = false;
  panel.update(sample, 7, 5);
  panel.update(sample, 12, 5);
  const DisplayConfig display{DisplayPanel::Ssd1306_128x32, 28, 29, 0x3D};
  const auto frame = panel.frame({}, display);
  TEST_ASSERT_TRUE(frame.stacked);
  TEST_ASSERT_EQUAL_STRING("KIVO MENU", frame.lines[0].c_str());
  TEST_ASSERT_EQUAL_STRING("> LIVE VIEW", frame.lines[1].c_str());
  TEST_ASSERT_TRUE(frame.lines[2].empty());
  std::uint32_t now = 12;
  for (unsigned i = 0; i < 4; ++i) {
    sample.encoderAHigh = false;
    panel.update(sample, ++now, 5);
    sample.encoderBHigh = false;
    panel.update(sample, ++now, 5);
    sample.encoderAHigh = true;
    panel.update(sample, ++now, 5);
    sample.encoderBHigh = true;
    panel.update(sample, ++now, 5);
  }
  TEST_ASSERT_EQUAL_STRING("> DEVICE INFO", panel.frame({}, display).lines[1].c_str());
  now += 20;
  sample.confirmPressed = true;
  panel.update(sample, now, 5);
  panel.update(sample, now + 5, 5);
  const auto info = panel.frame({}, display);
  TEST_ASSERT_EQUAL_STRING("SSD1306 128X32", info.lines[0].c_str());
  TEST_ASSERT_EQUAL_STRING("I2C 0X3D", info.lines[1].c_str());
}
}

int main() {
  UNITY_BEGIN();
  RUN_TEST(test_actual_panel_geometry_rejects_before_commit);
  RUN_TEST(test_render_failure_never_acknowledges_success);
  RUN_TEST(test_menu_preserves_remote_scene_and_restores_full_frame);
  RUN_TEST(test_panel_change_discards_old_scene_and_revision);
  RUN_TEST(test_transfer_failure_is_reported_once_and_disables_acceptance);
  RUN_TEST(test_unconfigured_or_failed_display_rejects_remote_commands);
  RUN_TEST(test_compact_menu_keeps_selection_visible_and_uses_actual_panel_info);
  return UNITY_END();
}
