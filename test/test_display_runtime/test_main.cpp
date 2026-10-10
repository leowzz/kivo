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

ControlPanelUpdate rotate(ControlPanel &panel, ControlPanelSample &sample,
                          std::uint32_t &now, bool clockwise = true) {
  ControlPanelUpdate result = ControlPanelUpdate::None;
  for (const auto state : clockwise ? std::array<unsigned, 4>{1, 0, 2, 3}
                                   : std::array<unsigned, 4>{2, 0, 1, 3}) {
    sample.encoderAHigh = state & 2;
    sample.encoderBHigh = state & 1;
    result = panel.update(sample, ++now, 5);
  }
  return result;
}

ControlPanelUpdate press(ControlPanel &panel, ControlPanelSample &sample,
                         std::uint32_t &now, bool back = false) {
  now += 20;
  if (back) sample.backPressed = true;
  else sample.encoderPressed = true;
  panel.update(sample, ++now, 5);
  now += 5;
  const auto result = panel.update(sample, now, 5);
  sample.backPressed = sample.encoderPressed = false;
  panel.update(sample, ++now, 5);
  now += 5;
  panel.update(sample, now, 5);
  return result;
}

void openRunCat(ControlPanel &panel, ControlPanelSample &sample, std::uint32_t &now) {
  panel.update(sample, now, 5);
  press(panel, sample, now);
  // Counterclockwise wraps directly from LIVE VIEW to RUNCAT.
  rotate(panel, sample, now, false);
  TEST_ASSERT_EQUAL(ControlPanelUpdate::Render, press(panel, sample, now));
}

void test_runcat_switches_wraps_and_returns_to_latest_live_scene() {
  for (const auto type : {DisplayPanel::Ssd1306_128x32, DisplayPanel::Sh1106_128x64}) {
    ControlPanel panel;
    ControlPanelSample sample;
    std::uint32_t now = 0;
    const DisplayConfig config{type, 28, 29};
    openRunCat(panel, sample, now);
    TEST_ASSERT_TRUE(panel.frame({}, config).runCat.has_value());
    const auto count = static_cast<unsigned>(RunCatAnimation::Count);
    for (unsigned i = 0; i < count; ++i) {
      TEST_ASSERT_EQUAL(ControlPanelUpdate::AnimationChanged, rotate(panel, sample, now));
      TEST_ASSERT_EQUAL_UINT8((i + 1) % count, panel.animation());
    }
    TEST_ASSERT_EQUAL(ControlPanelUpdate::AnimationChanged, rotate(panel, sample, now, false));
    TEST_ASSERT_EQUAL_UINT8(count - 1, panel.animation());

    FakeSurface surface;
    DisplayRuntime runtime(surface);
    configure(runtime, type);
    runtime.showInteractive(panel.frame({}, config));
    stage(runtime);
    send(runtime, "DISPLAY_COMMIT 1\n");
    TEST_ASSERT_EQUAL_UINT(0, surface.remoteFrames);
    TEST_ASSERT_EQUAL(ControlPanelUpdate::Render, press(panel, sample, now, true));
    TEST_ASSERT_FALSE(panel.frame({}, config).runCat.has_value());
    TEST_ASSERT_EQUAL(ControlPanelUpdate::Dismiss, press(panel, sample, now, true));
    runtime.dismissInteractive();
    TEST_ASSERT_EQUAL_UINT(1, surface.remoteFrames);
    TEST_ASSERT_TRUE(surface.lastFull);
    panel.reset();
    TEST_ASSERT_EQUAL_UINT8(count - 1, panel.animation());
  }
}

void test_runcat_uses_cpu_speed_and_expires_stale_data_across_clock_rollover() {
  ControlPanel panel;
  ControlPanelSample sample;
  std::uint32_t now = 0;
  openRunCat(panel, sample, now);
  const DisplayConfig display{};
  panel.tickAnimation(1000);
  panel.setCpuPercent(100, 1000);
  const auto phase = panel.frame({}, display).runCat->phase;
  TEST_ASSERT_FALSE(panel.tickAnimation(1049));
  TEST_ASSERT_TRUE(panel.tickAnimation(1050));
  TEST_ASSERT_EQUAL_UINT8((phase + 1) % runCatPhaseCount(RunCatAnimation::Cat),
                        panel.frame({}, display).runCat->phase);
  panel.tickAnimation(4000);
  TEST_ASSERT_FALSE(panel.frame({}, display).runCat->cpuPercent.has_value());
  TEST_ASSERT_EQUAL_UINT32(125, runCatFrameInterval(std::nullopt));
  TEST_ASSERT_EQUAL_UINT32(50, runCatFrameInterval(100));
  panel.setCpuPercent(50, UINT32_MAX - 1000);
  panel.tickAnimation(1000);
  TEST_ASSERT_EQUAL_UINT8(50, *panel.frame({}, display).runCat->cpuPercent);
  panel.tickAnimation(2000);
  TEST_ASSERT_FALSE(panel.frame({}, display).runCat->cpuPercent.has_value());
  panel.setAnimation(255);
  TEST_ASSERT_EQUAL_UINT8(0, panel.animation());
}

void test_cpu_command_validates_percentage_without_touching_a_scene_transaction() {
  for (const auto *line : {"DISPLAY_CPU -1", "DISPLAY_CPU 101", "DISPLAY_CPU", "DISPLAY_CPU 50 extra"})
    TEST_ASSERT_FALSE(parseHelperCommand(line).has_value());
  FakeSurface surface;
  DisplayRuntime runtime(surface);
  configure(runtime);
  stage(runtime);
  const auto command = parseHelperCommand("DISPLAY_CPU 100\n");
  TEST_ASSERT_EQUAL(HelperCommandKind::DisplayCpu, command->kind);
  TEST_ASSERT_EQUAL_UINT8(100, command->cpuPercent);
  TEST_ASSERT_FALSE(runtime.handle(*command).has_value());
  TEST_ASSERT_FALSE(runtime.malformed("DISPLAY_CPU 101").has_value());
  TEST_ASSERT_EQUAL_STRING("DISPLAY_OK 1\n", send(runtime, "DISPLAY_COMMIT 1\n")->c_str());
}

void test_each_runcat_animation_has_distinct_moving_frames() {
  const std::uint8_t expectedCounts[] = {5, 5, 7, 5, 6, 7, 10, 24, 15, 14};
  for (unsigned animation = 0; animation < static_cast<unsigned>(RunCatAnimation::Count); ++animation) {
    const auto type = static_cast<RunCatAnimation>(animation);
    const auto &first = runCatBitmap(type, 0);
    TEST_ASSERT_EQUAL_UINT8(expectedCounts[animation], runCatPhaseCount(type));
    TEST_ASSERT_TRUE(first != RunCatBitmap{});
    TEST_ASSERT_TRUE(first != runCatBitmap(type, 1));
    TEST_ASSERT_TRUE(first != runCatBitmap(type, 2));
    TEST_ASSERT_TRUE(first == runCatBitmap(type, expectedCounts[animation]));
    for (unsigned other = 0; other < animation; ++other)
      TEST_ASSERT_TRUE(first != runCatBitmap(static_cast<RunCatAnimation>(other), 0));

    // Long sequences must reach every original frame instead of wrapping at four.
    ControlPanel panel;
    ControlPanelSample sample;
    std::uint32_t now = 0;
    openRunCat(panel, sample, now);
    panel.setAnimation(animation);
    for (unsigned step = 1; step <= expectedCounts[animation]; ++step) {
      now += 125;
      TEST_ASSERT_TRUE(panel.tickAnimation(now));
      TEST_ASSERT_EQUAL_UINT8(step % expectedCounts[animation], panel.frame({}, {}).runCat->phase);
    }
  }
  TEST_ASSERT_TRUE(runCatBitmap(static_cast<RunCatAnimation>(255), 0) ==
                   runCatBitmap(RunCatAnimation::Cat, 0));
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
  RUN_TEST(test_runcat_switches_wraps_and_returns_to_latest_live_scene);
  RUN_TEST(test_runcat_uses_cpu_speed_and_expires_stale_data_across_clock_rollover);
  RUN_TEST(test_cpu_command_validates_percentage_without_touching_a_scene_transaction);
  RUN_TEST(test_each_runcat_animation_has_distinct_moving_frames);
  return UNITY_END();
}
