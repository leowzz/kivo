#include "U8g2Display.h"

#include <algorithm>
#include <array>

namespace {
const std::uint8_t *font(std::uint8_t id) {
  switch (id) {
    case 0: return u8g2_font_6x13_tf;
    case 1: return u8g2_font_9x18_tf;
    case 2: return u8g2_font_10x20_tf;
    default: return nullptr;
  }
}
// One tile per service call keeps display work bounded on a shared I2C bus.
constexpr std::size_t kRefreshDataBytes = 8;
}

void U8g2Display::stop() {
  if (ready_ && bus_.healthy()) screen_.setPowerSave(1);
  ready_ = false;
  bus_.release();
  reset();
}

bool U8g2Display::configure(const std::optional<DisplayConfig> &config) {
  stop();
  requested_ = config.has_value();
  if (!config) return true;
  if (!bus_.acquire(config->sda, config->scl)) return false;
  capabilities_ = displayCapabilities(config->panel);
  switch (config->panel) {
    case DisplayPanel::Ssd1306_128x32:
      u8g2_Setup_ssd1306_i2c_128x32_univision_f(screen_.getU8g2(), U8G2_R0,
          transfer, u8x8_gpio_and_delay_arduino);
      break;
    case DisplayPanel::Sh1106_128x64:
      u8g2_Setup_sh1106_i2c_128x64_noname_f(screen_.getU8g2(), U8G2_R0,
          transfer, u8x8_gpio_and_delay_arduino);
      break;
  }
  screen_.setUserPtr(&bus_);
  if (bus_.software()) {
    u8x8_SetPin_SW_I2C(screen_.getU8x8(), config->scl, config->sda, U8X8_PIN_NONE);
  } else {
    u8x8_SetPin_HW_I2C(screen_.getU8x8(), U8X8_PIN_NONE);
  }
  screen_.setI2CAddress(config->address << 1U);
  screen_.setBusClock(400000);
  // U8g2::begin clears the entire physical panel synchronously. Initialize
  // commands here, then let service refresh the framebuffer in bounded work.
  screen_.initDisplay();
  screen_.setPowerSave(0);
  if (!bus_.healthy()) {
    stop();
    return false;
  }
  ready_ = true;
  screen_.clearBuffer();
  dirty_.markPixels({0, 0, capabilities_.width, capabilities_.height});
  return true;
}

bool U8g2Display::renderLocal(const DisplayFrame &frame) {
  if (!ready_ || !bus_.healthy()) return !requested_;
  if (source_ == Source::Local && lastLocal_ == frame) return true;
  if (frame.runCat) {
    std::array<std::uint8_t, 1024> previous{};
    std::copy_n(screen_.getBufferPtr(), capabilities_.width * capabilities_.height / 8,
                previous.begin());
    // Reserve the rightmost four tiles for metrics. Artwork is already cropped
    // across the whole cycle and fitted to the 96x64 animation area.
    const DisplayRect bounds{0, 0, 96, capabilities_.height};
    const auto width = kRunCatWidth * bounds.height / kRunCatHeight;
    const auto left = (bounds.width - width) / 2;
    const bool animationOnly = source_ == Source::Local && lastLocal_ &&
        lastLocal_->runCat && lastLocal_->lines == frame.lines;
    if (animationOnly) {
      screen_.setDrawColor(0);
      screen_.drawBox(bounds.x, bounds.y, bounds.width, bounds.height);
      screen_.setDrawColor(1);
    } else {
      screen_.clearBuffer();
      screen_.setFont(font(0));
      screen_.setFontPosBaseline();
      const auto right = capabilities_.width - 2;
      screen_.drawStr(right - screen_.getStrWidth("CPU"), capabilities_.height / 2 - 2, "CPU");
      screen_.drawStr(right - screen_.getStrWidth(frame.lines[1].c_str()),
                      capabilities_.height / 2 + 12, frame.lines[1].c_str());
    }
    const auto &bitmap = runCatBitmap(frame.runCat->animation, frame.runCat->phase);
    for (unsigned y = 0; y < bounds.height; ++y)
      for (unsigned x = 0; x < width; ++x) {
        const auto sx = x * kRunCatWidth / width;
        const auto sy = y * kRunCatHeight / bounds.height;
        if (bitmap[sy * kRunCatStride + sx / 8] & (0x80U >> (sx % 8)))
          screen_.drawPixel(left + x, y);
      }
    // Re-marking the entire tall animation on every frame would starve its
    // lower tiles at high CPU usage. Queue only tiles whose pixels changed.
    const auto *buffer = screen_.getBufferPtr();
    for (unsigned ty = 0; ty < capabilities_.height / 8; ++ty)
      for (unsigned tx = 0; tx < capabilities_.width / 8; ++tx) {
        const auto offset = ty * capabilities_.width + tx * 8;
        if (!std::equal(previous.begin() + offset, previous.begin() + offset + 8, buffer + offset))
          dirty_.markPixels({static_cast<std::uint16_t>(tx * 8), static_cast<std::uint16_t>(ty * 8), 8, 8});
      }
    lastLocal_ = frame;
    source_ = Source::Local;
    return true;
  }
  screen_.clearBuffer();
  screen_.setFont(font(0));
  screen_.setFontPosBaseline();
  if (capabilities_.height == 64) {
    for (std::size_t index = 0; index < frame.lines.size(); ++index) {
      screen_.drawStr(0, 12 + index * 16, frame.lines[index].c_str());
    }
  } else if (frame.stacked) {
    screen_.drawStr(0, 12, frame.lines[0].c_str());
    screen_.drawStr(0, 28, frame.lines[1].c_str());
  } else {
    screen_.drawStr(0, 10, frame.lines[0].c_str());
    screen_.drawStr(0, 29, frame.lines[1].c_str());
    if (!frame.lines[2].empty()) {
      const auto width = screen_.getStrWidth(frame.lines[2].c_str());
      screen_.drawStr(width < capabilities_.width ? capabilities_.width - width : 0,
                      20, frame.lines[2].c_str());
    }
  }
  dirty_.markPixels({0, 0, capabilities_.width, capabilities_.height});
  lastLocal_ = frame;
  source_ = Source::Local;
  return true;
}

bool U8g2Display::renderRemote(const RemoteDisplayCommit &scene, bool full) {
  if (!ready_ || !bus_.healthy()) return !requested_;
  const bool redraw = full || source_ != Source::Remote;
  if (redraw) {
    screen_.clearBuffer();
  } else {
    screen_.setDrawColor(0);
    for (std::size_t i = 0; i < scene.dirtyCount; ++i) {
      const auto &rect = scene.dirtyBounds[i];
      screen_.drawBox(rect.x, rect.y, rect.width, rect.height);
    }
    screen_.setDrawColor(1);
  }
  screen_.setFontPosBaseline();
  for (std::size_t i = 0; i < scene.operationCount; ++i) {
    const auto &op = scene.operations[i];
    if (op.kind != DisplayOperationKind::Text) continue;
    const auto *selected = font(op.fontId);
    if (!selected) return false;
    screen_.setFont(selected);
    screen_.drawStr(op.x, op.baselineY, op.text.c_str());
  }
  if (redraw) {
    dirty_.markPixels({0, 0, capabilities_.width, capabilities_.height});
  } else {
    for (std::size_t i = 0; i < scene.dirtyCount; ++i) dirty_.markPixels(scene.dirtyBounds[i]);
  }
  lastLocal_.reset();
  source_ = Source::Remote;
  return true;
}

void U8g2Display::reset() {
  dirty_.clear();
  lastLocal_.reset();
  source_ = Source::None;
}

void U8g2Display::setBrightness(std::uint8_t percent) {
  if (ready_ && bus_.healthy()) {
    screen_.setContrast((std::min<std::uint16_t>(percent, 100) * 255U + 50U) / 100U);
  }
}

bool U8g2Display::service() {
  if (!ready_ || !bus_.healthy()) return !requested_;
  if (const auto run = dirty_.takeRun(kRefreshDataBytes)) {
    screen_.updateDisplayArea(run->tx, run->ty, run->tw, run->th);
  }
  return bus_.healthy();
}

std::uint8_t U8g2Display::transfer(u8x8_t *screen, std::uint8_t message,
                                  std::uint8_t size, void *data) {
  auto &bus = *static_cast<platform::I2cBus *>(u8x8_GetUserPtr(screen));
  if (bus.software()) return u8x8_byte_arduino_sw_i2c(screen, message, size, data);
  switch (message) {
    case U8X8_MSG_BYTE_INIT:
    case U8X8_MSG_BYTE_SET_DC:
      return 1;
    case U8X8_MSG_BYTE_START_TRANSFER:
      return bus.beginTransmission(u8x8_GetI2CAddress(screen) >> 1U);
    case U8X8_MSG_BYTE_SEND:
      return bus.write(static_cast<std::uint8_t *>(data), size);
    case U8X8_MSG_BYTE_END_TRANSFER:
      return bus.endTransmission();
    default:
      return 0;
  }
}
