#pragma once

#include <array>
#include <cstddef>
#include <cstdint>
#include <optional>

#include "BoardProfile.h"
#include "DisplayStatus.h"
#include "InputTopology.h"

namespace platform {
const BoardProfile &boardProfile();
void begin();
bool connected();
int available();
int read();
void write(const char *data, std::size_t size);
void flush();
using KeyboardKeycodes = std::array<std::uint8_t, 6>;
bool sendKeyboardChord(std::uint8_t modifiers, const KeyboardKeycodes &keys);
bool sendHotkey(std::uint8_t modifiers, std::uint8_t keycode);
bool sendConsumerControl(std::uint16_t usage);
void configureDisplay(const std::optional<OledConfig> &config);
void renderDisplay(const DisplayFrame &frame);
void showRandomKeyColor();
void clearKeyColor();
void delayMs(std::uint32_t milliseconds);
}  // namespace platform
