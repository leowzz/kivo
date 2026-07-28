#include <unity.h>

#include "GpioTriggerController.h"
#include "TriggerProtocol.h"

void setUp() {}
void tearDown() {}

void test_exposes_supported_gpio_inputs() {
  TEST_ASSERT_TRUE(GpioTriggerController::isSupportedPin(0));
  TEST_ASSERT_TRUE(GpioTriggerController::isSupportedPin(1));
  TEST_ASSERT_TRUE(GpioTriggerController::isSupportedPin(9));
  TEST_ASSERT_TRUE(GpioTriggerController::isSupportedPin(12));
  TEST_ASSERT_TRUE(GpioTriggerController::isSupportedPin(18));
  TEST_ASSERT_FALSE(GpioTriggerController::isSupportedPin(10));
  TEST_ASSERT_FALSE(GpioTriggerController::isSupportedPin(11));
  TEST_ASSERT_FALSE(GpioTriggerController::isSupportedPin(19));
}

void test_stable_edges_emit_once_after_debounce() {
  GpioTriggerController controller(0);

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
  GpioTriggerController controller(0);

  controller.updatePin(6, false, 0);
  const auto first = controller.updatePin(6, false, 30);
  TEST_ASSERT_TRUE(first.has_value());
  TEST_ASSERT_EQUAL(ResponseAction::Cleared,
                    controller.handleResponse(first->id, false));

  controller.updatePin(6, true, 40);
  controller.updatePin(6, true, 70);
  controller.updatePin(6, false, 80);
  const auto second = controller.updatePin(6, false, 110);

  TEST_ASSERT_TRUE(second.has_value());
  TEST_ASSERT_EQUAL_UINT32(3, second->id);
}

void test_tracks_pending_responses_per_gpio() {
  GpioTriggerController controller(0);

  controller.updatePin(6, false, 0);
  const auto first = controller.updatePin(6, false, 30);
  controller.updatePin(7, false, 40);
  const auto second = controller.updatePin(7, false, 70);

  TEST_ASSERT_TRUE(first.has_value());
  TEST_ASSERT_TRUE(second.has_value());
  TEST_ASSERT_EQUAL_UINT32(1, first->id);
  TEST_ASSERT_EQUAL_UINT32(2, second->id);
  TEST_ASSERT_EQUAL(ResponseAction::Execute,
                    controller.handleResponse(second->id, true));
  TEST_ASSERT_TRUE(controller.hasPendingEvent());
  TEST_ASSERT_EQUAL(ResponseAction::Cleared,
                    controller.handleResponse(first->id, false));
  TEST_ASSERT_FALSE(controller.hasPendingEvent());
}

void test_pending_responses_expire() {
  GpioTriggerController controller(0);

  controller.updatePin(6, false, 0);
  TEST_ASSERT_TRUE(controller.updatePin(6, false, 30).has_value());
  controller.expire(2029);
  TEST_ASSERT_TRUE(controller.hasPendingEvent());
  controller.expire(2030);
  TEST_ASSERT_FALSE(controller.hasPendingEvent());
}

void test_debounce_survives_millisecond_clock_rollover() {
  constexpr std::uint32_t kBeforeRollover = 0xFFFFFFF5U;
  GpioTriggerController controller(kBeforeRollover);

  TEST_ASSERT_FALSE(
      controller.updatePin(6, false, kBeforeRollover).has_value());
  TEST_ASSERT_FALSE(controller.updatePin(6, false, 0x00000012U).has_value());

  const auto event = controller.updatePin(6, false, 0x00000013U);
  TEST_ASSERT_TRUE(event.has_value());
  TEST_ASSERT_EQUAL_UINT8(6, event->gpio);
}

void test_serializes_input_state_events() {
  TEST_ASSERT_EQUAL_STRING(
      "STATE 42 6 DOWN\n",
      formatInputEvent(InputEvent{42, 6, InputState::Down}).c_str());
  TEST_ASSERT_EQUAL_STRING(
      "STATE 43 6 UP\n",
      formatInputEvent(InputEvent{43, 6, InputState::Up}).c_str());
}

void test_parses_paste_and_skip_responses() {
  const auto paste = parseHelperResponse("PASTE 42\n");
  TEST_ASSERT_TRUE(paste.has_value());
  TEST_ASSERT_EQUAL(HelperResponseKind::Paste, paste->kind);
  TEST_ASSERT_EQUAL_UINT32(42, paste->eventId);

  const auto skip = parseHelperResponse("SKIP 7\r\n");
  TEST_ASSERT_TRUE(skip.has_value());
  TEST_ASSERT_EQUAL(HelperResponseKind::Skip, skip->kind);
  TEST_ASSERT_EQUAL_UINT32(7, skip->eventId);
}

void test_rejects_malformed_responses() {
  TEST_ASSERT_FALSE(parseHelperResponse("PASTE\n").has_value());
  TEST_ASSERT_FALSE(parseHelperResponse("PASTE nope\n").has_value());
  TEST_ASSERT_FALSE(parseHelperResponse("PASTE 1 trailing\n").has_value());
  TEST_ASSERT_FALSE(parseHelperResponse("OTHER 1\n").has_value());
}

void test_parses_hotkey_response() {
  const auto response = parseHelperResponse("HOTKEY 42 10 14\n");
  TEST_ASSERT_TRUE(response.has_value());
  TEST_ASSERT_EQUAL(HelperResponseKind::Hotkey, response->kind);
  TEST_ASSERT_EQUAL_UINT32(42, response->eventId);
  TEST_ASSERT_EQUAL_UINT8(10, response->modifierMask);
  TEST_ASSERT_EQUAL_UINT8(14, response->keycode);
}

void test_rejects_malformed_hotkey_response() {
  TEST_ASSERT_FALSE(parseHelperResponse("HOTKEY 42 10\n").has_value());
  TEST_ASSERT_FALSE(parseHelperResponse("HOTKEY 42 256 14\n").has_value());
  TEST_ASSERT_FALSE(parseHelperResponse("HOTKEY 42 10 0\n").has_value());
  TEST_ASSERT_FALSE(parseHelperResponse("HOTKEY 42 10 165\n").has_value());
}

void test_discards_the_rest_of_an_overlong_physical_line() {
  ResponseLineBuffer lines(10);

  for (const char character : std::string("OVERLONG RESPONSEPASTE 42\n")) {
    TEST_ASSERT_FALSE(lines.push(character).has_value());
  }

  std::optional<std::string> response;
  for (const char character : std::string("PASTE 7\n")) {
    response = lines.push(character);
  }
  TEST_ASSERT_TRUE(response.has_value());
  TEST_ASSERT_EQUAL_STRING("PASTE 7\n", response->c_str());
}

void test_only_matching_paste_response_requests_keypress() {
  GpioTriggerController controller(0);
  controller.updatePin(6, false, 0);
  const auto event = controller.updatePin(6, false, 30);
  TEST_ASSERT_TRUE(event.has_value());

  TEST_ASSERT_EQUAL(ResponseAction::Ignored,
                    controller.handleResponse(event->id + 1, true));
  TEST_ASSERT_TRUE(controller.hasPendingEvent());
  TEST_ASSERT_EQUAL(ResponseAction::Execute,
                    controller.handleResponse(event->id, true));
  TEST_ASSERT_FALSE(controller.hasPendingEvent());
}

void test_matching_hotkey_response_requests_execution() {
  GpioTriggerController controller(0);
  controller.updatePin(6, false, 0);
  const auto event = controller.updatePin(6, false, 30);
  TEST_ASSERT_TRUE(event.has_value());
  TEST_ASSERT_EQUAL(ResponseAction::Execute,
                    controller.handleResponse(event->id, true));
  TEST_ASSERT_FALSE(controller.hasPendingEvent());
}

void test_matching_skip_response_clears_without_keypress() {
  GpioTriggerController controller(0);
  controller.updatePin(6, false, 0);
  const auto event = controller.updatePin(6, false, 30);
  TEST_ASSERT_TRUE(event.has_value());

  TEST_ASSERT_EQUAL(ResponseAction::Cleared,
                    controller.handleResponse(event->id, false));
  TEST_ASSERT_FALSE(controller.hasPendingEvent());
}

int main(int, char **) {
  UNITY_BEGIN();
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
