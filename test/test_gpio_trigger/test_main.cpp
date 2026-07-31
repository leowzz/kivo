#include <unity.h>

#include "BoardProfile.h"
#include "GpioTriggerController.h"
#include "Handshake.h"
#include "InputTopology.h"
#include "TriggerProtocol.h"

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

void test_board_profiles_enforce_exact_safe_pins() {
  TEST_ASSERT_TRUE(kLuatOsEsp32S3Aio.supports(18));
  TEST_ASSERT_FALSE(kLuatOsEsp32S3Aio.supports(19));
  TEST_ASSERT_TRUE(kVccGndYdRp2040.supports(0));
  TEST_ASSERT_TRUE(kVccGndYdRp2040.supports(22));
  for (std::uint8_t pin = 23; pin <= 29; ++pin) {
    TEST_ASSERT_FALSE(kVccGndYdRp2040.supports(pin));
  }

  TopologyBuilder rp2040(kVccGndYdRp2040);
  TEST_ASSERT_TRUE(rp2040.begin(1, 30));
  TEST_ASSERT_TRUE(rp2040.addDirect(1, 0, {0, 22}));

  TopologyBuilder reserved(kVccGndYdRp2040);
  TEST_ASSERT_TRUE(reserved.begin(1, 30));
  TEST_ASSERT_FALSE(reserved.addDirect(1, 0, {23}));
}

void test_formats_protocol_v3_hello_with_board_and_build() {
  TEST_ASSERT_EQUAL_STRING(
      "HELLO 3 rp2040 vccgnd-yd-rp2040 0.1.0+gabc1234 23 "
      "0 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 19 20 21 22\n",
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

  const auto commit = parseHelperCommand("CONFIG_COMMIT 3\n");
  TEST_ASSERT_TRUE(commit.has_value());
  TEST_ASSERT_EQUAL(HelperCommandKind::ConfigCommit, commit->kind);
}

void test_parses_extended_registered_board_pin_domain() {
  const auto direct = parseHelperCommand("CONFIG_DIRECT 3 0 5 10 11 19 20 22\n");

  TEST_ASSERT_TRUE(direct.has_value());
  TEST_ASSERT_EQUAL_UINT8(5, direct->pins.size());
  TEST_ASSERT_EQUAL_UINT8(10, direct->pins[0]);
  TEST_ASSERT_EQUAL_UINT8(22, direct->pins[4]);

  TopologyBuilder esp32(kLuatOsEsp32S3Aio);
  TEST_ASSERT_TRUE(esp32.begin(3, 30));
  TEST_ASSERT_FALSE(esp32.addDirect(3, 0, direct->pins));

  TopologyBuilder rp2040(kVccGndYdRp2040);
  TEST_ASSERT_TRUE(rp2040.begin(3, 30));
  TEST_ASSERT_TRUE(rp2040.addDirect(3, 0, direct->pins));
}

void test_parser_defers_unsupported_pins_to_board_validation() {
  const auto direct = parseHelperCommand("CONFIG_DIRECT 3 0 2 23 29\n");

  TEST_ASSERT_TRUE(direct.has_value());
  TEST_ASSERT_EQUAL_UINT8(2, direct->pins.size());
  TEST_ASSERT_EQUAL_UINT8(23, direct->pins[0]);
  TEST_ASSERT_EQUAL_UINT8(29, direct->pins[1]);

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
  TEST_ASSERT_FALSE(parseHelperCommand("PASTE 9 0 2\n").has_value());
  TEST_ASSERT_FALSE(parseHelperCommand("HOTKEY 9 2 1 0 40\n").has_value());
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

void test_rp2040_learning_accepts_gpio22_and_rejects_gpio23() {
  GpioTriggerController esp32(kLuatOsEsp32S3Aio, 0);
  TEST_ASSERT_FALSE(esp32.beginLearning(4, {22}, 0));

  GpioTriggerController rp2040(kVccGndYdRp2040, 0);
  TEST_ASSERT_TRUE(rp2040.beginLearning(4, {22}, 0));
  TEST_ASSERT_TRUE(rp2040.endLearning(4, 1));
  TEST_ASSERT_FALSE(rp2040.beginLearning(5, {23}, 1));
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

int main(int, char **) {
  UNITY_BEGIN();
  RUN_TEST(test_commits_complete_matrix_topology_atomically);
  RUN_TEST(test_board_profiles_enforce_exact_safe_pins);
  RUN_TEST(test_formats_protocol_v3_hello_with_board_and_build);
  RUN_TEST(test_rejects_empty_firmware_build_id);
  RUN_TEST(test_rejects_whitespace_in_firmware_build_id);
  RUN_TEST(test_contact_edge_reports_unordered_pair_once_after_debounce);
  RUN_TEST(test_parses_runtime_configuration_commands);
  RUN_TEST(test_parses_extended_registered_board_pin_domain);
  RUN_TEST(test_parser_defers_unsupported_pins_to_board_validation);
  RUN_TEST(test_parses_learning_and_ordered_action_commands);
  RUN_TEST(test_rejects_malformed_runtime_commands);
  RUN_TEST(test_action_steps_are_strictly_ordered);
  RUN_TEST(test_learning_reports_contact_and_restores_runtime_topology);
  RUN_TEST(test_rp2040_learning_accepts_gpio22_and_rejects_gpio23);
  RUN_TEST(test_suppresses_new_contact_that_closes_a_ghost_cycle);
  RUN_TEST(test_exposes_supported_gpio_inputs);
  RUN_TEST(test_stable_edges_emit_once_after_debounce);
  RUN_TEST(test_release_rearms_pin_for_a_later_press);
  RUN_TEST(test_tracks_pending_responses_per_gpio);
  RUN_TEST(test_pending_responses_expire);
  RUN_TEST(test_debounce_survives_millisecond_clock_rollover);
  RUN_TEST(test_serializes_input_state_events);
  RUN_TEST(test_parses_paste_and_skip_responses);
  RUN_TEST(test_rejects_malformed_responses);
  RUN_TEST(test_parses_hotkey_response);
  RUN_TEST(test_rejects_malformed_hotkey_response);
  RUN_TEST(test_discards_the_rest_of_an_overlong_physical_line);
  RUN_TEST(test_only_matching_paste_response_requests_keypress);
  RUN_TEST(test_matching_hotkey_response_requests_execution);
  RUN_TEST(test_matching_skip_response_clears_without_keypress);
  return UNITY_END();
}
