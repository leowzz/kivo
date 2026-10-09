#pragma once

#include <cstdint>

enum class DisplayPanel { Ssd1306_128x32, Sh1106_128x64 };

struct DisplayCapabilities {
  std::uint16_t width = 128;
  std::uint16_t height = 32;
  std::uint8_t maxFontId = 2;
};

constexpr DisplayCapabilities displayCapabilities(DisplayPanel panel) {
  return {128, static_cast<std::uint16_t>(
                   panel == DisplayPanel::Sh1106_128x64 ? 64 : 32), 2};
}

struct DisplayConfig {
  DisplayPanel panel = DisplayPanel::Ssd1306_128x32;
  std::uint8_t sda = 0;
  std::uint8_t scl = 0;
  std::uint8_t address = 0x3C;
};
