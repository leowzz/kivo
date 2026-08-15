#pragma once

#include <array>
#include <cstddef>
#include <cstdint>
#include <optional>

#include "BoardProfile.h"
#include "DisplayStatus.h"
#include "InputTopology.h"
#include "RemoteDisplay.h"

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
bool configureDisplay(const std::optional<OledConfig> &config);
void setDisplayBrightness(std::uint8_t percent);
bool renderLocalDisplay(const DisplayFrame &frame);
bool renderRemoteDisplay(const RemoteDisplayCommit &scene, bool fullRedraw);
void resetRemoteDisplay();
void serviceDisplay();
void showRandomKeyColor();
void clearKeyColor();
void delayMs(std::uint32_t milliseconds);
}  // namespace platform
