#include <unity.h>

#include <cstdint>
#include <string>

#include "platform/SerialWrite.h"

void setUp() {}
void tearDown() {}

void test_backpressure_preserves_the_whole_line_in_fifo_sized_chunks() {
  std::uint32_t now = 0;
  std::string sent;
  const auto result = platform::writeSerialWithDeadline(
      "HELLO\n", 6, 10, [] { return true; },
      [&] { return now < 2 ? 0 : 2; },
      [&](const char *data, std::size_t size) {
        TEST_ASSERT_LESS_OR_EQUAL_UINT32(2, size);
        sent.append(data, size);
        return size;
      },
      [&] { return now; }, [&] { ++now; });
  TEST_ASSERT_EQUAL(platform::SerialWriteResult::Complete, result);
  TEST_ASSERT_EQUAL_STRING("HELLO\n", sent.c_str());
}

void test_full_fifo_times_out_even_across_clock_wrap() {
  for (const std::uint32_t start : {0U, UINT32_MAX - 2}) {
    std::uint32_t now = start;
    unsigned writes = 0;
    const auto result = platform::writeSerialWithDeadline(
        "HELLO\n", 6, 5, [] { return true; }, [] { return 0; },
        [&](const char *, std::size_t size) { ++writes; return size; },
        [&] { return now; }, [&] { ++now; });
    TEST_ASSERT_EQUAL(platform::SerialWriteResult::TimedOut, result);
    TEST_ASSERT_EQUAL_UINT32(5, now - start);
    TEST_ASSERT_EQUAL_UINT32(0, writes);
  }
}

void test_disconnect_during_backpressure_does_not_request_a_reboot() {
  std::uint32_t now = 0;
  const auto result = platform::writeSerialWithDeadline(
      "HELLO\n", 6, 5, [&] { return now < 2; }, [] { return 0; },
      [](const char *, std::size_t size) { return size; },
      [&] { return now; }, [&] { ++now; });
  TEST_ASSERT_EQUAL(platform::SerialWriteResult::Disconnected, result);
  TEST_ASSERT_EQUAL_UINT32(2, now);
}

void test_partial_writes_cannot_extend_the_deadline_forever() {
  std::uint32_t now = 0;
  std::string sent;
  const auto result = platform::writeSerialWithDeadline(
      "HELLO\n", 6, 3, [] { return true; }, [] { return 6; },
      [&](const char *data, std::size_t) { sent += *data; return 1; },
      [&] { return now; }, [&] { ++now; });
  TEST_ASSERT_EQUAL(platform::SerialWriteResult::TimedOut, result);
  TEST_ASSERT_EQUAL_STRING("HEL", sent.c_str());
}

int main(int, char **) {
  UNITY_BEGIN();
  RUN_TEST(test_backpressure_preserves_the_whole_line_in_fifo_sized_chunks);
  RUN_TEST(test_full_fifo_times_out_even_across_clock_wrap);
  RUN_TEST(test_disconnect_during_backpressure_does_not_request_a_reboot);
  RUN_TEST(test_partial_writes_cannot_extend_the_deadline_forever);
  return UNITY_END();
}
