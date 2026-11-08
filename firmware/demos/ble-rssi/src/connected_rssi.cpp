#include <Arduino.h>
#include <BLEDevice.h>
#include <BLEHIDDevice.h>
#include <BLESecurity.h>
#include <BLEServer.h>
#include <cstring>
#include <esp_gap_ble_api.h>
#include <freertos/FreeRTOS.h>
#include <freertos/queue.h>

namespace {
constexpr char kDeviceName[] = "Kivo RSSI Demo";
constexpr uint32_t kReadIntervalMs = 1000;
constexpr uint32_t kReadTimeoutMs = 3000;

// Consumer Control only. This demo never notifies an input report.
uint8_t reportMap[] = {
    0x05, 0x0C, 0x09, 0x01, 0xA1, 0x01, 0x85, 0x01,
    0x15, 0x00, 0x26, 0xFF, 0x03, 0x19, 0x00, 0x2A,
    0xFF, 0x03, 0x75, 0x10, 0x95, 0x01, 0x81, 0x00, 0xC0};

struct LinkState {
  bool connected = false;
  bool restartAdvertising = false;
  bool pending = false;
  bool timeoutReported = false;
  uint8_t peer[6] = {};
  uint32_t generation = 0;
  uint32_t requestedAtMs = 0;
};
struct Sample {
  uint8_t peer[6];
  uint32_t generation;
  uint32_t observedAtMs;
  int status;
  int rssi;
};

portMUX_TYPE stateMux = portMUX_INITIALIZER_UNLOCKED;
LinkState connectionState;
QueueHandle_t samples = nullptr;

LinkState snapshot() {
  portENTER_CRITICAL(&stateMux);
  LinkState copy = connectionState;
  portEXIT_CRITICAL(&stateMux);
  return copy;
}

void printPeer(const uint8_t *peer) {
  Serial.printf("%02X:%02X:%02X:%02X:%02X:%02X", peer[0], peer[1], peer[2],
                peer[3], peer[4], peer[5]);
}

class LinkCallbacks : public BLEServerCallbacks {
  void onConnect(BLEServer *, esp_ble_gatts_cb_param_t *param) override {
    portENTER_CRITICAL(&stateMux);
    memcpy(connectionState.peer, param->connect.remote_bda, sizeof(connectionState.peer));
    connectionState.connected = true;
    connectionState.restartAdvertising = false;
    connectionState.pending = false;
    connectionState.timeoutReported = false;
    connectionState.requestedAtMs = 0;
    ++connectionState.generation;
    portEXIT_CRITICAL(&stateMux);
  }
  void onDisconnect(BLEServer *) override {
    portENTER_CRITICAL(&stateMux);
    connectionState.connected = false;
    connectionState.pending = false;
    connectionState.restartAdvertising = true;
    ++connectionState.generation;
    portEXIT_CRITICAL(&stateMux);
  }
};
LinkCallbacks linkCallbacks;

void gapEvent(esp_gap_ble_cb_event_t event, esp_ble_gap_cb_param_t *param) {
  if (event != ESP_GAP_BLE_READ_RSSI_COMPLETE_EVT) {
    return;
  }
  const auto &result = param->read_rssi_cmpl;
  Sample sample{};
  portENTER_CRITICAL(&stateMux);
  const bool expected = connectionState.connected && connectionState.pending &&
                        memcmp(connectionState.peer, result.remote_addr, sizeof(connectionState.peer)) == 0;
  if (expected) {
    connectionState.pending = false;
    connectionState.timeoutReported = false;
    sample.generation = connectionState.generation;
  }
  portEXIT_CRITICAL(&stateMux);
  if (!expected) {
    return;
  }
  memcpy(sample.peer, result.remote_addr, sizeof(sample.peer));
  sample.observedAtMs = millis();
  sample.status = result.status;
  sample.rssi = result.rssi;
  xQueueSend(samples, &sample, 0);
}
}  // namespace

void setup() {
  Serial.begin(115200);
  delay(1200);
  samples = xQueueCreate(16, sizeof(Sample));
  if (samples == nullptr) {
    Serial.println("ERROR: could not allocate RSSI queue");
    return;
  }
  BLEDevice::init(kDeviceName);
  BLEDevice::setCustomGapHandler(gapEvent);
  BLEDevice::setEncryptionLevel(ESP_BLE_SEC_ENCRYPT);
  static BLESecurity security;
  security.setAuthenticationMode(ESP_LE_AUTH_REQ_SC_BOND);
  security.setCapability(ESP_IO_CAP_NONE);
  security.setInitEncryptionKey(ESP_BLE_ENC_KEY_MASK | ESP_BLE_ID_KEY_MASK);
  security.setRespEncryptionKey(ESP_BLE_ENC_KEY_MASK | ESP_BLE_ID_KEY_MASK);
  BLEServer *server = BLEDevice::createServer();
  server->setCallbacks(&linkCallbacks);
  BLEHIDDevice *hid = new BLEHIDDevice(server);
  hid->manufacturer()->setValue("Kivo");
  hid->pnp(0x02, 0x303A, 0x0001, 0x0100);
  hid->hidInfo(0x00, 0x02);
  hid->reportMap(reportMap, sizeof(reportMap));
  uint8_t neutralReport[] = {0, 0};
  hid->inputReport(1)->setValue(neutralReport, sizeof(neutralReport));
  hid->startServices();
  hid->setBatteryLevel(100);  // USB-powered; the service must exist before notify().
  BLEAdvertising *advertising = BLEDevice::getAdvertising();
  advertising->setAppearance(GENERIC_HID);
  advertising->addServiceUUID(hid->hidService()->getUUID());
  advertising->setScanResponse(true);
  advertising->start();
  Serial.printf("ADVERTISING name=\"%s\" profile=consumer_control\n", kDeviceName);
  Serial.println("Pair this device in the phone's Bluetooth settings to read connected RSSI.");
}

void loop() {
  if (samples == nullptr) {
    delay(1000);
    return;
  }
  const uint32_t now = millis();
  LinkState current = snapshot();
  static uint32_t reportedGeneration = 0;
  if (current.generation != reportedGeneration) {
    reportedGeneration = current.generation;
    Serial.printf("LINK state=%s peer=", current.connected ? "CONNECTED" : "DISCONNECTED");
    printPeer(current.peer);
    Serial.println();
  }
  if (current.restartAdvertising && !current.connected) {
    portENTER_CRITICAL(&stateMux);
    const bool restart = connectionState.restartAdvertising && !connectionState.connected;
    if (restart) {
      connectionState.restartAdvertising = false;
    }
    portEXIT_CRITICAL(&stateMux);
    if (restart) {
      BLEDevice::startAdvertising();
      Serial.printf("ADVERTISING name=\"%s\"\n", kDeviceName);
    }
  }
  Sample sample{};
  while (xQueueReceive(samples, &sample, 0) == pdTRUE) {
    const LinkState latest = snapshot();
    if (!latest.connected || sample.generation != latest.generation) {
      continue;
    }
    if (sample.status == ESP_BT_STATUS_SUCCESS && sample.rssi != 127) {
      Serial.printf("RSSI t_ms=%lu peer=", static_cast<unsigned long>(sample.observedAtMs));
      printPeer(sample.peer);
      Serial.printf(" rssi_dbm=%d source=connection\n", sample.rssi);
    } else {
      Serial.printf("RSSI_UNAVAILABLE status=%d\n", sample.status);
    }
  }
  if (current.connected && current.pending && !current.timeoutReported &&
      now - current.requestedAtMs >= kReadTimeoutMs) {
    portENTER_CRITICAL(&stateMux);
    const bool timedOut = connectionState.generation == current.generation && connectionState.pending &&
                          connectionState.requestedAtMs == current.requestedAtMs;
    if (timedOut) {
      connectionState.timeoutReported = true;
    }
    portEXIT_CRITICAL(&stateMux);
    if (timedOut) {
      Serial.println("RSSI_TIMEOUT waiting_for_completion_or_disconnect");
    }
  }
  if (current.connected && !current.pending && now - current.requestedAtMs >= kReadIntervalMs) {
    portENTER_CRITICAL(&stateMux);
    const bool request = connectionState.connected && !connectionState.pending && connectionState.generation == current.generation;
    if (request) {
      connectionState.pending = true;
      connectionState.requestedAtMs = now;
    }
    portEXIT_CRITICAL(&stateMux);
    if (request) {
      const esp_err_t result = esp_ble_gap_read_rssi(current.peer);
      if (result != ESP_OK) {
        portENTER_CRITICAL(&stateMux);
        if (connectionState.generation == current.generation) {
          connectionState.pending = false;
        }
        portEXIT_CRITICAL(&stateMux);
        Serial.printf("RSSI_REQUEST_FAILED error=%d\n", result);
      }
    }
  }
  static uint32_t lastStatusAtMs = 0;
  if (now - lastStatusAtMs >= 2000) {
    lastStatusAtMs = now;
    Serial.printf("STATUS state=%s\n", current.connected ? "CONNECTED" : "ADVERTISING");
  }
  delay(10);
}
