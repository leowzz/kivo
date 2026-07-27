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

void test_stable_low_emits_one_event_after_debounce() {
  GpioTriggerController controller(0);

  TEST_ASSERT_FALSE(controller.updatePin(6, false, 1000).has_value());
  TEST_ASSERT_FALSE(controller.updatePin(6, false, 1029).has_value());

  const auto event = controller.updatePin(6, false, 1030);
  TEST_ASSERT_TRUE(event.has_value());
  TEST_ASSERT_EQUAL_UINT32(1, event->id);
  TEST_ASSERT_EQUAL_UINT8(6, event->gpio);
  TEST_ASSERT_TRUE(controller.hasPendingEvent());

  TEST_ASSERT_FALSE(controller.updatePin(6, false, 2000).has_value());
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
  TEST_ASSERT_EQUAL_UINT32(2, second->id);
}

void test_pending_event_blocks_other_pins_until_timeout() {
  GpioTriggerController controller(0);

  controller.updatePin(6, false, 0);
  const auto first = controller.updatePin(6, false, 30);
  TEST_ASSERT_TRUE(first.has_value());

  controller.updatePin(7, false, 100);
  TEST_ASSERT_FALSE(controller.updatePin(7, false, 130).has_value());
  controller.expire(2029);
  TEST_ASSERT_TRUE(controller.hasPendingEvent());
  controller.expire(2030);
  TEST_ASSERT_FALSE(controller.hasPendingEvent());

  controller.updatePin(7, true, 2040);
  controller.updatePin(7, true, 2070);
  controller.updatePin(7, false, 2080);
  const auto second = controller.updatePin(7, false, 2110);
  TEST_ASSERT_TRUE(second.has_value());
  TEST_ASSERT_EQUAL_UINT8(7, second->gpio);
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

void test_serializes_press_event() {
  const std::string line = formatPressEvent(PressEvent{42, 6});
  TEST_ASSERT_EQUAL_STRING("PRESS 42 6\n", line.c_str());
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

void test_only_matching_paste_response_requests_keypress() {
  GpioTriggerController controller(0);
  controller.updatePin(6, false, 0);
  const auto event = controller.updatePin(6, false, 30);
  TEST_ASSERT_TRUE(event.has_value());

  TEST_ASSERT_EQUAL(ResponseAction::Ignored,
                    controller.handleResponse(event->id + 1, true));
  TEST_ASSERT_TRUE(controller.hasPendingEvent());
  TEST_ASSERT_EQUAL(ResponseAction::Paste,
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
  RUN_TEST(test_stable_low_emits_one_event_after_debounce);
  RUN_TEST(test_release_rearms_pin_for_a_later_press);
  RUN_TEST(test_pending_event_blocks_other_pins_until_timeout);
  RUN_TEST(test_debounce_survives_millisecond_clock_rollover);
  RUN_TEST(test_serializes_press_event);
  RUN_TEST(test_parses_paste_and_skip_responses);
  RUN_TEST(test_rejects_malformed_responses);
  RUN_TEST(test_only_matching_paste_response_requests_keypress);
  RUN_TEST(test_matching_skip_response_clears_without_keypress);
  return UNITY_END();
}
