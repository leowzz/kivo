#include <unity.h>

#include "LedController.h"

void setUp() {}
void tearDown() {}

void test_starts_with_led_a_on_and_led_b_steady_on() {
  LedController controller(0);

  const LedOutputs outputs = controller.update(0, true);

  TEST_ASSERT_TRUE(outputs.ledA);
  TEST_ASSERT_TRUE(outputs.ledB);
}

void test_led_a_completes_three_flashes_per_second_without_drift() {
  LedController controller(0);

  TEST_ASSERT_TRUE(controller.update(166666, true).ledA);
  TEST_ASSERT_FALSE(controller.update(166667, true).ledA);
  TEST_ASSERT_TRUE(controller.update(333334, true).ledA);
  TEST_ASSERT_FALSE(controller.update(833335, true).ledA);
  TEST_ASSERT_TRUE(controller.update(1000002, true).ledA);
  TEST_ASSERT_TRUE(controller.update(1000002, true).ledB);
}

void test_stable_low_swaps_roles_once_after_debounce() {
  LedController controller(0);

  controller.update(1000, false);
  TEST_ASSERT_TRUE(controller.update(30999, false).ledA);

  const LedOutputs switched = controller.update(31000, false);
  TEST_ASSERT_TRUE(switched.ledA);
  TEST_ASSERT_TRUE(switched.ledB);

  const LedOutputs held = controller.update(231000, false);
  TEST_ASSERT_TRUE(held.ledA);
  TEST_ASSERT_FALSE(held.ledB);
}

void test_bounce_does_not_switch_and_stable_release_rearms_input() {
  LedController controller(0);

  controller.update(1000, false);
  controller.update(10000, true);
  controller.update(15000, false);
  controller.update(20000, true);
  TEST_ASSERT_TRUE(controller.update(50000, true).ledA);

  controller.update(60000, false);
  TEST_ASSERT_TRUE(controller.update(90000, false).ledA);

  controller.update(100000, true);
  controller.update(130000, true);
  controller.update(140000, false);
  const LedOutputs switchedBack = controller.update(170000, false);

  TEST_ASSERT_TRUE(switchedBack.ledA);
  TEST_ASSERT_TRUE(switchedBack.ledB);
  TEST_ASSERT_FALSE(controller.update(336667, false).ledA);
  TEST_ASSERT_TRUE(controller.update(336667, false).ledB);
}

int main(int, char **) {
  UNITY_BEGIN();
  RUN_TEST(test_starts_with_led_a_on_and_led_b_steady_on);
  RUN_TEST(test_led_a_completes_three_flashes_per_second_without_drift);
  RUN_TEST(test_stable_low_swaps_roles_once_after_debounce);
  RUN_TEST(test_bounce_does_not_switch_and_stable_release_rearms_input);
  return UNITY_END();
}
