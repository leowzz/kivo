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

int main(int, char **) {
  UNITY_BEGIN();
  RUN_TEST(test_starts_with_led_a_on_and_led_b_steady_on);
  RUN_TEST(test_led_a_completes_three_flashes_per_second_without_drift);
  return UNITY_END();
}
