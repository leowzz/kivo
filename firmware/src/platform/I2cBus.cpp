#include "I2cBus.h"

#if defined(ARDUINO_ARCH_RP2040)
#include "Rp2040I2cRouting.h"
#endif

namespace {
struct BusLease {
  TwoWire *wire;
  std::uint8_t sda = 0;
  std::uint8_t scl = 0;
  std::uint16_t users = 0;
};
BusLease buses[] = {{&Wire}
#if defined(ARDUINO_ARCH_RP2040)
                   , {&Wire1}
#endif
};
constexpr std::uint32_t kClockHz = 400000;
}

namespace platform {
bool I2cBus::acquire(std::uint8_t sda, std::uint8_t scl) {
  release();
  healthy_ = true;
  std::size_t index = 0;
#if defined(ARDUINO_ARCH_RP2040)
  const auto selection = selectRp2040I2cRouting(sda, scl);
  if (selection == Rp2040I2cRouting::Software) {
    acquired_ = true;
    return true;
  }
  index = selection == Rp2040I2cRouting::I2c1 ? 1 : 0;
#endif
  auto &bus = buses[index];
  if (bus.users && (bus.sda != sda || bus.scl != scl)) return false;
  if (!bus.users) {
#if defined(ARDUINO_ARCH_RP2040)
    if (!bus.wire->setSDA(sda) || !bus.wire->setSCL(scl)) return false;
    bus.wire->begin();
#else
    if (!bus.wire->begin(sda, scl)) return false;
#endif
    bus.wire->setClock(kClockHz);
    bus.sda = sda;
    bus.scl = scl;
  }
  ++bus.users;
  wire_ = bus.wire;
  acquired_ = true;
  return true;
}

void I2cBus::release() {
  if (acquired_ && wire_) {
    for (auto &bus : buses) {
      if (bus.wire == wire_ && --bus.users == 0) bus.wire->end();
    }
  }
  wire_ = nullptr;
  acquired_ = false;
}

bool I2cBus::beginTransmission(std::uint8_t address) {
  if (!acquired_ || !wire_ || !healthy_) return false;
  wire_->beginTransmission(address);
  return true;
}

bool I2cBus::write(const std::uint8_t *data, std::size_t size) {
  if (!acquired_ || !wire_ || !healthy_) return false;
  healthy_ = wire_->write(data, size) == size;
  return healthy_;
}

bool I2cBus::endTransmission() {
  if (!acquired_ || !wire_) return false;
  healthy_ = (wire_->endTransmission() == 0) && healthy_;
  return healthy_;
}
}  // namespace platform
