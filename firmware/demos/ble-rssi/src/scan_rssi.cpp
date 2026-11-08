#include <Arduino.h>
#include <BLEAdvertisedDevice.h>
#include <BLEDevice.h>
#include <BLEScan.h>
#include <algorithm>
#include <atomic>
#include <cctype>
#include <cstring>
#include <freertos/FreeRTOS.h>
#include <freertos/queue.h>

namespace {

// Configure the scan target here. The connected demo needs no target address.
constexpr char kTargetName[] = "phone";
constexpr char kTargetMac[] = "";
constexpr char kCandidateName[] = "xiaomi";
constexpr uint32_t kStatusIntervalMs = 2000;
constexpr uint32_t kStaleAfterMs = 5000;

struct Sample {
  uint32_t observedAtMs;
  uint8_t address[6];
  int rssi;
  bool target;
  uint8_t addressType;
  char name[64];
};

QueueHandle_t samples = nullptr;
std::atomic<uint32_t> scanReports{0};
std::atomic<uint32_t> droppedSamples{0};
uint32_t targetPackets = 0;
uint32_t lastSeenAtMs = 0;
uint32_t lastStatusAtMs = 0;
bool scanStarted = false;
bool targetConfigured = false;

std::string lowercase(std::string text) {
  std::transform(text.begin(), text.end(), text.begin(),
                 [](unsigned char c) { return static_cast<char>(std::tolower(c)); });
  return text;
}

bool validTargetAddress() {
  if (strlen(kTargetMac) != 17) {
    return false;
  }
  for (size_t i = 0; i < 17; ++i) {
    if (i % 3 == 2) {
      if (kTargetMac[i] != ':') {
        return false;
      }
    } else if (!std::isxdigit(static_cast<unsigned char>(kTargetMac[i]))) {
      return false;
    }
  }
  return true;
}

class TargetCallbacks : public BLEAdvertisedDeviceCallbacks {
  void onResult(BLEAdvertisedDevice device) override {
    ++scanReports;
    auto address = device.getAddress();
    const auto &bytes = *address.getNative();
    const std::string name = device.getName();
    const bool target = targetConfigured && address.toString() == lowercase(kTargetMac);
#ifdef BLE_RSSI_DISCOVERY
    const std::string lowerName = lowercase(name);
    const bool candidate = !name.empty() && kCandidateName[0] != '\0' &&
                           lowerName.find(lowercase(kCandidateName)) != std::string::npos;
    if (!target && !candidate) {
      return;
    }
#else
    if (!target) {
      return;
    }
#endif

    Sample sample{};
    sample.observedAtMs = millis();
    memcpy(sample.address, bytes, sizeof(sample.address));
    sample.rssi = device.getRSSI();
    sample.target = target;
    sample.addressType = device.getAddressType();
    snprintf(sample.name, sizeof(sample.name), "%s", name.c_str());
    for (char &c : sample.name) {
      if (c == '\n' || c == '\r') {
        c = ' ';
      }
    }
    // The BLE callback never waits for USB output.
    if (xQueueSend(samples, &sample, 0) != pdTRUE) {
      ++droppedSamples;
    }
  }
};

TargetCallbacks callbacks;

}  // namespace

void setup() {
  Serial.begin(115200);
  delay(1200);
  Serial.printf("BLE_RSSI_DEMO target_mac=%s target_name=\"%s\"\n", kTargetMac, kTargetName);
  Serial.println("RSSI is in dBm; values closer to zero indicate a stronger signal.");
  targetConfigured = validTargetAddress();
#ifndef BLE_RSSI_DISCOVERY
  if (!targetConfigured) {
    Serial.println("ERROR: set kTargetMac in scan_rssi.cpp to AA:BB:CC:DD:EE:FF format");
    return;
  }
#endif

  samples = xQueueCreate(32, sizeof(Sample));
  if (samples == nullptr) {
    Serial.println("ERROR: could not allocate sample queue");
    return;
  }

  BLEDevice::init("");
  BLEScan *scan = BLEDevice::getScan();
  // Repeated advertisements are essential for continuously measuring RSSI.
  // Parse advertisements and scan responses for the phone's local name.
  // Repeated callbacks do not retain a scan-result list.
  scan->setAdvertisedDeviceCallbacks(&callbacks, true, true);
  scan->setActiveScan(true);
#ifdef BLE_RSSI_DISCOVERY
  Serial.printf("MODE active_scan=true candidate_name=\"%s\"\n", kCandidateName);
#endif
  scan->setInterval(100);
  scan->setWindow(100);
  // Select the asynchronous overload; duration zero scans indefinitely.
  scanStarted = scan->start(0, static_cast<void (*)(BLEScanResults)>(nullptr), false);
  if (!scanStarted) {
    Serial.println("ERROR: BLE scan did not start; reset the board to retry");
    return;
  }
  Serial.printf("SCANNING target_mac=%s\n", kTargetMac);
}

void demo() {
#ifndef BLE_RSSI_DISCOVERY
  if (!targetConfigured) {
    const uint32_t now = millis();
    if (now - lastStatusAtMs >= kStatusIntervalMs) {
      lastStatusAtMs = now;
      Serial.println("STATUS state=TARGET_NOT_CONFIGURED");
    }
    return;
  }
#endif
  if (samples == nullptr) {
    return;
  }

  Sample sample{};
  while (xQueueReceive(samples, &sample, 0) == pdTRUE) {
    if (sample.target) {
      ++targetPackets;
      lastSeenAtMs = sample.observedAtMs;
    }
    Serial.printf(
        "%s t_ms=%lu mac=%02X:%02X:%02X:%02X:%02X:%02X rssi_dbm=%d",
        sample.target ? "RSSI" : "CANDIDATE",
        static_cast<unsigned long>(sample.observedAtMs),
        sample.address[0], sample.address[1], sample.address[2],
        sample.address[3], sample.address[4], sample.address[5], sample.rssi);
    Serial.printf(" address_type=%u name=\"%s\"", sample.addressType, sample.name);
    Serial.println();
  }

  const uint32_t now = millis();
  if (now - lastStatusAtMs < kStatusIntervalMs) {
    return;
  }
  lastStatusAtMs = now;
  Serial.printf("STATUS scan_reports=%lu target_packets=%lu dropped=%lu ",
                static_cast<unsigned long>(scanReports.load()),
                static_cast<unsigned long>(targetPackets),
                static_cast<unsigned long>(droppedSamples.load()));
  if (!scanStarted) {
    Serial.println("state=SCAN_NOT_RUNNING");
  } else if (targetPackets == 0) {
    Serial.printf("state=NO_TARGET_YET mac=%s\n", kTargetMac);
  } else {
    const uint32_t age = now - lastSeenAtMs;
    Serial.printf("state=%s last_seen_ms=%lu\n",
                  age < kStaleAfterMs ? "RECEIVING" : "NO_RECENT_PACKET",
                  static_cast<unsigned long>(age));
  }
}

void loop() {
  demo();
  delay(10);
}
