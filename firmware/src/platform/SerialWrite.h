#pragma once

#include <algorithm>
#include <cstddef>
#include <cstdint>

namespace platform {
enum class SerialWriteResult { Complete, Disconnected, TimedOut };

// Never enter the CDC library's unbounded wait for FIFO space. The caller is
// the sole writer, so a write no larger than the advertised capacity cannot
// fill the FIFO midway through that call.
template <typename Connected, typename Capacity, typename Write,
          typename Clock, typename Pause>
SerialWriteResult writeSerialWithDeadline(
    const char *data, std::size_t size, std::uint32_t timeoutMs,
    Connected connected, Capacity capacity, Write write, Clock now, Pause pause) {
  const std::uint32_t started = now();
  while (size) {
    if (!connected()) return SerialWriteResult::Disconnected;
    if (static_cast<std::uint32_t>(now() - started) >= timeoutMs)
      return SerialWriteResult::TimedOut;
    const auto available = capacity();
    if (available > 0) {
      const auto count = write(data, std::min(size, static_cast<std::size_t>(available)));
      data += count;
      size -= count;
    }
    if (size) pause();
  }
  return SerialWriteResult::Complete;
}
}  // namespace platform
