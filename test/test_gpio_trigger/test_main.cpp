#include <unity.h>

#include "BoardProfile.h"
#include "DisplayStatus.h"
#include "ActionRunController.h"
#include "GpioTriggerController.h"
#include "Handshake.h"
#include "InputTopology.h"
#include "KeyActivityIndicator.h"
#include "StandaloneDebugTopology.h"
#include "TriggerProtocol.h"
#include "platform/HidReportTransport.h"
#include "platform/Rp2040OledBus.h"

void setUp() {}
void tearDown() {}

GpioTriggerController directController(std::uint32_t startMs) {
  TopologyBuilder builder(kLuatOsEsp32S3Aio);
  builder.begin(1, 30);
  builder.addDirect(1, 0, {0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 12, 13, 14, 15, 16,
                           17, 18});
  GpioTriggerController controller(kLuatOsEsp32S3Aio, startMs);
  controller.configure(*builder.commit(1), startMs);
  return controller;
}

void test_commits_complete_matrix_topology_atomically() {
  TopologyBuilder builder(kLuatOsEsp32S3Aio);
  TEST_ASSERT_TRUE(builder.begin(7, 30));
  TEST_ASSERT_TRUE(builder.addMatrix(7, 0, {1, 2}, {12, 13}));

  const auto topology = builder.commit(7);

  TEST_ASSERT_TRUE(topology.has_value());
  TEST_ASSERT_EQUAL_UINT32(7, topology->revision);
  TEST_ASSERT_EQUAL_UINT8(2, topology->matrices[0].rows.size());
}

void test_runtime_topology_counts_direct_and_matrix_keys() {
  TopologyBuilder builder(kVccGndYdRp2040);
  TEST_ASSERT_TRUE(builder.begin(8, 30));
  TEST_ASSERT_TRUE(builder.addDirect(8, 0, {0, 1}));
  TEST_ASSERT_TRUE(builder.addMatrix(8, 1, {2, 3}, {4, 5}));

  const auto topology = builder.commit(8);

  TEST_ASSERT_TRUE(topology.has_value());
  TEST_ASSERT_EQUAL_UINT32(6, topology->keyCount());
}

void test_board_profiles_enforce_exact_safe_pins() {
  TEST_ASSERT_TRUE(kLuatOsEsp32S3Aio.supports(18));
  TEST_ASSERT_FALSE(kLuatOsEsp32S3Aio.supports(19));
  TEST_ASSERT_TRUE(kVccGndYdRp2040.supports(0));
  TEST_ASSERT_TRUE(kVccGndYdRp2040.supports(22));
  for (std::uint8_t pin = 23; pin <= 25; ++pin) {
    TEST_ASSERT_FALSE(kVccGndYdRp2040.supports(pin));
  }
  for (std::uint8_t pin = 26; pin <= 29; ++pin) {
    TEST_ASSERT_TRUE(kVccGndYdRp2040.supports(pin));
  }

  TopologyBuilder rp2040(kVccGndYdRp2040);
  TEST_ASSERT_TRUE(rp2040.begin(1, 30));
  TEST_ASSERT_TRUE(rp2040.addDirect(1, 0, {0, 22, 26, 27, 28, 29}));

  TopologyBuilder reserved(kVccGndYdRp2040);
  TEST_ASSERT_TRUE(reserved.begin(1, 30));
  TEST_ASSERT_FALSE(reserved.addDirect(1, 0, {23}));
}

void test_board_profiles_report_oled_capability() {
  TEST_ASSERT_FALSE(kLuatOsEsp32S3Aio.supportsOled);
  TEST_ASSERT_TRUE(kVccGndYdRp2040.supportsOled);
}

void test_rp2040_standalone_debug_topology_matches_keyboard_wiring() {
  const auto topology = makeRp2040StandaloneDebugTopology(kVccGndYdRp2040);

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
      makeRp2040StandaloneDebugTopology(kLuatOsEsp32S3Aio).has_value());
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

void test_formats_protocol_v6_hello_with_board_and_build() {
  TEST_ASSERT_EQUAL_STRING(
      "HELLO 6 rp2040 vccgnd-yd-rp2040 0.1.0+gabc1234 27 "
      "0 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 19 20 21 22 26 "
      "27 28 29\n",
      formatHello(kVccGndYdRp2040, "0.1.0+gabc1234").c_str());
}

void test_rejects_empty_firmware_build_id() {
  TEST_ASSERT_EQUAL_STRING("", formatHello(kVccGndYdRp2040, "").c_str());
}

void test_rejects_whitespace_in_firmware_build_id() {
  TEST_ASSERT_EQUAL_STRING("",
                           formatHello(kVccGndYdRp2040, "0.1.0 test").c_str());
}

void test_contact_edge_reports_unordered_pair_once_after_debounce() {
  TopologyBuilder builder(kLuatOsEsp32S3Aio);
  TEST_ASSERT_TRUE(builder.begin(7, 30));
  TEST_ASSERT_TRUE(builder.addMatrix(7, 0, {1, 2}, {12, 13}));
  GpioTriggerController controller(kLuatOsEsp32S3Aio, 0);
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

  const auto commit = parseHelperCommand("CONFIG_COMMIT 3\n");
  TEST_ASSERT_TRUE(commit.has_value());
  TEST_ASSERT_EQUAL(HelperCommandKind::ConfigCommit, commit->kind);
}

void test_oled_configuration_requires_supported_distinct_safe_pins() {
  TopologyBuilder esp32(kLuatOsEsp32S3Aio);
  TEST_ASSERT_TRUE(esp32.begin(1, 30));
  TEST_ASSERT_FALSE(esp32.addOled(1, 4, 5));

  TopologyBuilder rp2040(kVccGndYdRp2040);
  TEST_ASSERT_TRUE(rp2040.begin(1, 30));
  TEST_ASSERT_FALSE(rp2040.addOled(1, 4, 4));
  TEST_ASSERT_FALSE(rp2040.addOled(1, 4, 23));
  TEST_ASSERT_TRUE(rp2040.addOled(1, 28, 29));
  const auto topology = rp2040.commit(1);
  TEST_ASSERT_TRUE(topology.has_value());
  TEST_ASSERT_TRUE(topology->oled.has_value());
  TEST_ASSERT_EQUAL_UINT8(28, topology->oled->sda);
  TEST_ASSERT_EQUAL_UINT8(29, topology->oled->scl);
}

void test_oled_pins_are_reserved_when_oled_command_arrives_first() {
  TopologyBuilder builder(kVccGndYdRp2040);
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
  TopologyBuilder builder(kVccGndYdRp2040);
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

void test_parses_extended_registered_board_pin_domain() {
  const auto direct = parseHelperCommand(
      "CONFIG_DIRECT 3 0 9 10 11 19 20 22 26 27 28 29\n");

  TEST_ASSERT_TRUE(direct.has_value());
  TEST_ASSERT_EQUAL_UINT8(9, direct->pins.size());
  TEST_ASSERT_EQUAL_UINT8(10, direct->pins[0]);
  TEST_ASSERT_EQUAL_UINT8(29, direct->pins[8]);

  TopologyBuilder esp32(kLuatOsEsp32S3Aio);
  TEST_ASSERT_TRUE(esp32.begin(3, 30));
  TEST_ASSERT_FALSE(esp32.addDirect(3, 0, direct->pins));

  TopologyBuilder rp2040(kVccGndYdRp2040);
  TEST_ASSERT_TRUE(rp2040.begin(3, 30));
  TEST_ASSERT_TRUE(rp2040.addDirect(3, 0, direct->pins));
}

void test_parser_defers_unsupported_pins_to_board_validation() {
  const auto direct = parseHelperCommand("CONFIG_DIRECT 3 0 3 23 24 25\n");

  TEST_ASSERT_TRUE(direct.has_value());
  TEST_ASSERT_EQUAL_UINT8(3, direct->pins.size());
  TEST_ASSERT_EQUAL_UINT8(23, direct->pins[0]);
  TEST_ASSERT_EQUAL_UINT8(25, direct->pins[2]);

  TopologyBuilder esp32(kLuatOsEsp32S3Aio);
  TEST_ASSERT_TRUE(esp32.begin(3, 30));
  TEST_ASSERT_FALSE(esp32.addDirect(3, 0, direct->pins));

  TopologyBuilder rp2040(kVccGndYdRp2040);
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
  TEST_ASSERT_FALSE(parseHelperCommand("PASTE 9 0 2\n").has_value());
  TEST_ASSERT_FALSE(parseHelperCommand("HOTKEY 9 2 1 0 40\n").has_value());
  TEST_ASSERT_FALSE(parseHelperCommand("DELAY 9 1 1 0\n").has_value());
  TEST_ASSERT_FALSE(parseHelperCommand("DELAY 9 1 1 60001\n").has_value());
  TEST_ASSERT_FALSE(parseHelperCommand("MEDIA 9 1 1 1\n").has_value());
  TEST_ASSERT_FALSE(parseHelperCommand("HOST 9 1 1 trailing\n").has_value());
  TEST_ASSERT_FALSE(parseHelperCommand(std::string(256, 'x')).has_value());
}

void test_action_steps_are_strictly_ordered() {
  auto controller = directController(0);
  controller.updatePin(6, false, 0);
  const auto event = controller.updatePin(6, false, 30);
  TEST_ASSERT_TRUE(event.has_value());

  TEST_ASSERT_EQUAL(ResponseAction::Ignored,
                    controller.acceptStep(event->id, 2, 2, true, 40));
  TEST_ASSERT_EQUAL(ResponseAction::Execute,
                    controller.acceptStep(event->id, 1, 2, true, 40));
  TEST_ASSERT_TRUE(controller.hasPendingEvent());
  controller.expire(2039);
  TEST_ASSERT_TRUE(controller.hasPendingEvent());
  TEST_ASSERT_EQUAL(ResponseAction::Execute,
                    controller.acceptStep(event->id, 2, 2, true, 2039));
  TEST_ASSERT_FALSE(controller.hasPendingEvent());
}

void test_long_delay_can_keep_a_pending_action_sequence_alive() {
  auto controller = directController(0);
  controller.updatePin(6, false, 0);
  const auto event = controller.updatePin(6, false, 30);
  TEST_ASSERT_TRUE(event.has_value());
  TEST_ASSERT_EQUAL(ResponseAction::Execute,
                    controller.acceptStep(event->id, 1, 2, true, 40));

  TEST_ASSERT_TRUE(controller.keepPendingEventAlive(event->id, 2000));
  controller.expire(3999);
  TEST_ASSERT_TRUE(controller.hasPendingEvent());
  controller.expire(4000);
  TEST_ASSERT_FALSE(controller.hasPendingEvent());
}

void test_repeated_same_input_keeps_both_pending_event_ids() {
  auto controller = directController(0);
  controller.updatePin(6, false, 0);
  const auto first = controller.updatePin(6, false, 30);
  TEST_ASSERT_TRUE(first.has_value());

  controller.updatePin(6, true, 40);
  controller.updatePin(6, true, 70);
  controller.updatePin(6, false, 80);
  const auto second = controller.updatePin(6, false, 110);
  TEST_ASSERT_TRUE(second.has_value());
  TEST_ASSERT_NOT_EQUAL(first->id, second->id);

  TEST_ASSERT_EQUAL(ResponseAction::Execute,
                    controller.acceptStep(first->id, 1, 1, true, 120));
  TEST_ASSERT_TRUE(controller.hasPendingEvent());
  TEST_ASSERT_EQUAL(ResponseAction::Execute,
                    controller.acceptStep(second->id, 1, 1, true, 121));
  TEST_ASSERT_FALSE(controller.hasPendingEvent());
}

void test_learning_reports_contact_and_restores_runtime_topology() {
  TopologyBuilder builder(kLuatOsEsp32S3Aio);
  builder.begin(7, 30);
  builder.addDirect(7, 0, {6});
  GpioTriggerController controller(kLuatOsEsp32S3Aio, 0);
  controller.configure(*builder.commit(7), 0);

  TEST_ASSERT_FALSE(controller.beginLearning(4, {1, 10, 12}, 0));
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

void test_rp2040_learning_accepts_gpio29_and_rejects_gpio23() {
  GpioTriggerController esp32(kLuatOsEsp32S3Aio, 0);
  TEST_ASSERT_FALSE(esp32.beginLearning(4, {29}, 0));

  GpioTriggerController rp2040(kVccGndYdRp2040, 0);
  TEST_ASSERT_TRUE(rp2040.beginLearning(4, {29}, 0));
  TEST_ASSERT_TRUE(rp2040.endLearning(4, 1));
  TEST_ASSERT_FALSE(rp2040.beginLearning(5, {23}, 1));
}

void test_learning_rejects_active_oled_pins() {
  TopologyBuilder builder(kVccGndYdRp2040);
  TEST_ASSERT_TRUE(builder.begin(7, 30));
  TEST_ASSERT_TRUE(builder.addDirect(7, 0, {6}));
  TEST_ASSERT_TRUE(builder.addOled(7, 4, 5));
  GpioTriggerController controller(kVccGndYdRp2040, 0);
  controller.configure(*builder.commit(7), 0);

  TEST_ASSERT_FALSE(controller.beginLearning(8, {4, 7}, 0));
  TEST_ASSERT_FALSE(controller.beginLearning(8, {5, 7}, 0));
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
  TopologyBuilder builder(kLuatOsEsp32S3Aio);
  builder.begin(1, 1);
  builder.addMatrix(1, 0, {1, 2}, {12, 13});
  GpioTriggerController controller(kLuatOsEsp32S3Aio, 0);
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
  GpioTriggerController controller(kLuatOsEsp32S3Aio);
  TEST_ASSERT_TRUE(controller.isSupportedPin(0));
  TEST_ASSERT_TRUE(controller.isSupportedPin(1));
  TEST_ASSERT_TRUE(controller.isSupportedPin(9));
  TEST_ASSERT_TRUE(controller.isSupportedPin(12));
  TEST_ASSERT_TRUE(controller.isSupportedPin(18));
  TEST_ASSERT_FALSE(controller.isSupportedPin(10));
  TEST_ASSERT_FALSE(controller.isSupportedPin(11));
  TEST_ASSERT_FALSE(controller.isSupportedPin(19));
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
  TEST_ASSERT_TRUE(controller.hasPendingEvent());
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
  TEST_ASSERT_EQUAL(ResponseAction::Cleared,
                    controller.acceptStep(first->id, 0, 0, false, 31));

  controller.updatePin(6, true, 40);
  controller.updatePin(6, true, 70);
  controller.updatePin(6, false, 80);
  const auto second = controller.updatePin(6, false, 110);

  TEST_ASSERT_TRUE(second.has_value());
  TEST_ASSERT_EQUAL_UINT32(3, second->id);
}

void test_tracks_pending_responses_per_gpio() {
  auto controller = directController(0);

  controller.updatePin(6, false, 0);
  const auto first = controller.updatePin(6, false, 30);
  controller.updatePin(7, false, 40);
  const auto second = controller.updatePin(7, false, 70);

  TEST_ASSERT_TRUE(first.has_value());
  TEST_ASSERT_TRUE(second.has_value());
  TEST_ASSERT_EQUAL_UINT32(1, first->id);
  TEST_ASSERT_EQUAL_UINT32(2, second->id);
  TEST_ASSERT_EQUAL(ResponseAction::Execute,
                    controller.acceptStep(second->id, 1, 1, true, 71));
  TEST_ASSERT_TRUE(controller.hasPendingEvent());
  TEST_ASSERT_EQUAL(ResponseAction::Cleared,
                    controller.acceptStep(first->id, 0, 0, false, 72));
  TEST_ASSERT_FALSE(controller.hasPendingEvent());
}

void test_pending_responses_expire() {
  auto controller = directController(0);

  controller.updatePin(6, false, 0);
  TEST_ASSERT_TRUE(controller.updatePin(6, false, 30).has_value());
  controller.expire(2029);
  TEST_ASSERT_TRUE(controller.hasPendingEvent());
  controller.expire(2030);
  TEST_ASSERT_FALSE(controller.hasPendingEvent());
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
  TEST_ASSERT_EQUAL_UINT32(42, paste->eventId);

  const auto skip = parseHelperCommand("SKIP 7\r\n");
  TEST_ASSERT_TRUE(skip.has_value());
  TEST_ASSERT_EQUAL(HelperCommandKind::Skip, skip->kind);
  TEST_ASSERT_EQUAL_UINT32(7, skip->eventId);
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
  TEST_ASSERT_EQUAL_UINT32(42, response->eventId);
  TEST_ASSERT_EQUAL_UINT8(10, response->modifierMask);
  TEST_ASSERT_EQUAL_UINT8(14, response->keycode);
}

void test_rejects_malformed_hotkey_response() {
  TEST_ASSERT_FALSE(parseHelperCommand("HOTKEY 42 1 1 10\n").has_value());
  TEST_ASSERT_FALSE(parseHelperCommand("HOTKEY 42 1 1 256 14\n").has_value());
  TEST_ASSERT_FALSE(parseHelperCommand("HOTKEY 42 1 1 10 0\n").has_value());
  TEST_ASSERT_FALSE(parseHelperCommand("HOTKEY 42 1 1 10 165\n").has_value());
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

void test_discards_the_rest_of_an_overlong_physical_line() {
  ResponseLineBuffer lines(16);

  for (const char character : std::string("OVERLONG RESPONSE PASTE 42 1 1\n")) {
    TEST_ASSERT_FALSE(lines.push(character).has_value());
  }

  std::optional<std::string> response;
  for (const char character : std::string("PASTE 7 1 1\n")) {
    response = lines.push(character);
  }
  TEST_ASSERT_TRUE(response.has_value());
  TEST_ASSERT_EQUAL_STRING("PASTE 7 1 1\n", response->c_str());
}

void test_only_matching_paste_response_requests_keypress() {
  auto controller = directController(0);
  controller.updatePin(6, false, 0);
  const auto event = controller.updatePin(6, false, 30);
  TEST_ASSERT_TRUE(event.has_value());

  TEST_ASSERT_EQUAL(ResponseAction::Ignored,
                    controller.acceptStep(event->id + 1, 1, 1, true, 31));
  TEST_ASSERT_TRUE(controller.hasPendingEvent());
  TEST_ASSERT_EQUAL(ResponseAction::Execute,
                    controller.acceptStep(event->id, 1, 1, true, 31));
  TEST_ASSERT_FALSE(controller.hasPendingEvent());
}

void test_matching_hotkey_response_requests_execution() {
  auto controller = directController(0);
  controller.updatePin(6, false, 0);
  const auto event = controller.updatePin(6, false, 30);
  TEST_ASSERT_TRUE(event.has_value());
  TEST_ASSERT_EQUAL(ResponseAction::Execute,
                    controller.acceptStep(event->id, 1, 1, true, 31));
  TEST_ASSERT_FALSE(controller.hasPendingEvent());
}

void test_matching_skip_response_clears_without_keypress() {
  auto controller = directController(0);
  controller.updatePin(6, false, 0);
  const auto event = controller.updatePin(6, false, 30);
  TEST_ASSERT_TRUE(event.has_value());

  TEST_ASSERT_EQUAL(ResponseAction::Cleared,
                    controller.acceptStep(event->id, 0, 0, false, 31));
  TEST_ASSERT_FALSE(controller.hasPendingEvent());
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
  RUN_TEST(test_commits_complete_matrix_topology_atomically);
  RUN_TEST(test_runtime_topology_counts_direct_and_matrix_keys);
  RUN_TEST(test_board_profiles_enforce_exact_safe_pins);
  RUN_TEST(test_board_profiles_report_oled_capability);
  RUN_TEST(test_rp2040_standalone_debug_topology_matches_keyboard_wiring);
  RUN_TEST(test_rp2040_oled_selects_hardware_i2c_when_pin_roles_match);
  RUN_TEST(test_rp2040_oled_falls_back_to_software_i2c_for_arbitrary_safe_pins);
  RUN_TEST(test_formats_protocol_v6_hello_with_board_and_build);
  RUN_TEST(test_rejects_empty_firmware_build_id);
  RUN_TEST(test_rejects_whitespace_in_firmware_build_id);
  RUN_TEST(test_contact_edge_reports_unordered_pair_once_after_debounce);
  RUN_TEST(test_parses_runtime_configuration_commands);
  RUN_TEST(test_oled_configuration_requires_supported_distinct_safe_pins);
  RUN_TEST(test_oled_pins_are_reserved_when_oled_command_arrives_first);
  RUN_TEST(test_oled_pins_are_reserved_when_input_command_arrives_first);
  RUN_TEST(test_parses_extended_registered_board_pin_domain);
  RUN_TEST(test_parser_defers_unsupported_pins_to_board_validation);
  RUN_TEST(test_parses_learning_and_ordered_action_commands);
  RUN_TEST(test_rejects_malformed_runtime_commands);
  RUN_TEST(test_action_steps_are_strictly_ordered);
  RUN_TEST(test_long_delay_can_keep_a_pending_action_sequence_alive);
  RUN_TEST(test_repeated_same_input_keeps_both_pending_event_ids);
  RUN_TEST(test_learning_reports_contact_and_restores_runtime_topology);
  RUN_TEST(test_rp2040_learning_accepts_gpio29_and_rejects_gpio23);
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
  RUN_TEST(test_tracks_pending_responses_per_gpio);
  RUN_TEST(test_pending_responses_expire);
  RUN_TEST(test_debounce_survives_millisecond_clock_rollover);
  RUN_TEST(test_serializes_input_state_events);
  RUN_TEST(test_parses_paste_and_skip_responses);
  RUN_TEST(test_rejects_malformed_responses);
  RUN_TEST(test_parses_hotkey_response);
  RUN_TEST(test_rejects_malformed_hotkey_response);
  RUN_TEST(test_parses_v6_chord_and_rejects_malformed_chords);
  RUN_TEST(test_v6_run_starts_on_step_one_and_is_independent_of_input_ids);
  RUN_TEST(test_v6_run_cancel_keepalive_and_expiry);
  RUN_TEST(test_discards_the_rest_of_an_overlong_physical_line);
  RUN_TEST(test_only_matching_paste_response_requests_keypress);
  RUN_TEST(test_matching_hotkey_response_requests_execution);
  RUN_TEST(test_matching_skip_response_clears_without_keypress);
  RUN_TEST(test_hid_hotkey_waits_for_press_and_release_report_slots);
  RUN_TEST(test_hid_consumer_control_waits_for_press_and_release_report_slots);
  return UNITY_END();
}
