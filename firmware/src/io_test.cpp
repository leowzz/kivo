#include <Arduino.h>

#include "IoTestProtocol.h"
#include "TriggerProtocol.h"

#if defined(ARDUINO_ARCH_RP2040)
#include <Adafruit_TinyUSB.h>
static const BoardProfile &board = kYdRp2040;
#define ioSerial Serial
#elif defined(ARDUINO_ARCH_ESP32)
#include <USB.h>
#include <USBCDC.h>
static const BoardProfile &board = kYdEsp32S3;
static USBCDC ioSerial;
#else
#error Unsupported I/O test board
#endif

static IoTestProtocol protocol(board);
static ResponseLineBuffer lines(64);
static bool wasConnected = false;

static void configureInput(std::uint8_t pin, IoInputMode mode) {
  const auto arduinoMode = mode == IoInputMode::PullUp ? INPUT_PULLUP
                        : mode == IoInputMode::PullDown ? INPUT_PULLDOWN : INPUT;
  pinMode(pin, arduinoMode);
}

void setup() {
  protocol.reset(configureInput);
#if defined(ARDUINO_ARCH_RP2040)
  TinyUSBDevice.setID(0x2e8a, 0x102e);
  TinyUSBDevice.setManufacturerDescriptor("Kivo");
  TinyUSBDevice.setProductDescriptor("Kivo I/O Test RP2040");
  ioSerial.begin(115200);
#else
  USB.VID(0x303a);
  USB.PID(0x4002);
  USB.manufacturerName("Kivo");
  USB.productName("Kivo I/O Test ESP32-S3");
  ioSerial.begin(115200);
  USB.begin();
#endif
}

void loop() {
  const bool connected = static_cast<bool>(ioSerial);
  if (!connected) {
    if (wasConnected) {
      protocol.reset(configureInput);
      lines = ResponseLineBuffer(64);
    }
    wasConnected = false;
    delay(1);
    return;
  }
  wasConnected = true;
  while (ioSerial.available()) {
    const auto line = lines.push(static_cast<char>(ioSerial.read()));
    if (!line || line->overflow) continue;
    auto command = std::string_view(line->line);
    if (!command.empty() && command.back() == '\n') command.remove_suffix(1);
    if (!command.empty() && command.back() == '\r') command.remove_suffix(1);
    const auto response = protocol.handle(command, KIVO_FIRMWARE_BUILD_ID,
        [](std::uint8_t pin) { return digitalRead(pin) != LOW; }, configureInput);
    ioSerial.write(reinterpret_cast<const std::uint8_t *>(response.data()), response.size());
    ioSerial.flush();
  }
  delay(1);
}
