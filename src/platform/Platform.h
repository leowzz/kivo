#pragma once

#include <cstddef>
#include <cstdint>

#include "BoardProfile.h"

namespace platform {
const BoardProfile &boardProfile();
void begin();
bool connected();
int available();
int read();
void write(const char *data, std::size_t size);
void flush();
void sendHotkey(std::uint8_t modifiers, std::uint8_t keycode);
void delayMs(std::uint32_t milliseconds);
}  // namespace platform
