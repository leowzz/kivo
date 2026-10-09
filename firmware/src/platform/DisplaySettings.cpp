#include <EEPROM.h>
#include <algorithm>
#include "Platform.h"

namespace {
constexpr std::uint8_t kDisplayBrightnessMagic = 0x4B;
constexpr std::uint8_t kMinimumDisplayBrightnessPercent = 5;
constexpr std::uint8_t kMaximumDisplayBrightnessPercent = 100;
struct PersistedDisplaySettings {
  std::uint8_t magic;
  std::uint8_t brightnessPercent;
  std::uint8_t checksum;
  std::uint8_t reserved;
};
static_assert(sizeof(PersistedDisplaySettings) == 4);
constexpr std::size_t kDisplaySettingsEepromSize =
    sizeof(PersistedDisplaySettings);
std::uint8_t displaySettingsChecksum(
    const PersistedDisplaySettings &settings) {
  return static_cast<std::uint8_t>(settings.magic ^ settings.brightnessPercent ^
                                   0xA5U);
}

bool validDisplaySettings(const PersistedDisplaySettings &settings) {
  return settings.magic == kDisplayBrightnessMagic &&
         settings.brightnessPercent >= kMinimumDisplayBrightnessPercent &&
         settings.brightnessPercent <= kMaximumDisplayBrightnessPercent &&
         settings.checksum == displaySettingsChecksum(settings);
}

}

namespace platform {
std::uint8_t loadDisplayBrightness() {
  EEPROM.begin(kDisplaySettingsEepromSize);
  PersistedDisplaySettings settings{};
  EEPROM.get(0, settings);
  return validDisplaySettings(settings) ? settings.brightnessPercent
                                        : kMaximumDisplayBrightnessPercent;
}

void saveDisplayBrightness(std::uint8_t percent) {
  const auto clamped = std::min<std::uint16_t>(
      std::max<std::uint16_t>(percent, kMinimumDisplayBrightnessPercent),
      kMaximumDisplayBrightnessPercent);
  PersistedDisplaySettings settings{
      kDisplayBrightnessMagic,
      static_cast<std::uint8_t>(clamped),
      0,
      0,
  };
  settings.checksum = displaySettingsChecksum(settings);
  EEPROM.put(0, settings);
  EEPROM.commit();
}

}
