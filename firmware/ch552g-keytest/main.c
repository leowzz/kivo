#include <Arduino.h>
#include "usb/USBCDC.h"
#include "usb/USBhandler.h"

// Scan IDs 1..16 follow Ver202; the independent RST key is ID 17.
// Never drive P3.4 (unknown L1 circuit) or change EEPROM/config words.
__idata __at(0x08) volatile uint32_t timer0_overflow_count = 0;
__idata __at(0x0C) volatile uint8_t timer0_overflow_count_5th_byte = 0;
void Timer0Interrupt(void) __interrupt(INT_NO_TMR0) __using(1);
void DeviceUSBInterrupt(void) __interrupt(INT_NO_USB) { USBInterrupt(); }

extern volatile __bit UpPoint2BusyFlag;
extern volatile __xdata uint8_t controlLineState;
extern volatile __xdata uint8_t UsbConfig;
extern __xdata uint8_t usbWritePointer;
extern __xdata uint8_t keyboardIdle;
extern __xdata __at(148) uint8_t Ep3Buffer[];
static volatile __bit keyboardBusy;

void USB_EP3_IN(void) {
  UEP3_T_LEN = 0;
  UEP3_CTRL = (UEP3_CTRL & ~MASK_UEP_T_RES) | UEP_T_RES_NAK;
  keyboardBusy = 0;
}

static __xdata uint8_t debounce[17];
static __xdata uint32_t stableKeys;
static __xdata uint32_t rawKeys;
static __xdata uint8_t hidQueue[32], eventQueue[32];
static __xdata uint8_t hidHead, hidTail, eventHead, eventTail;
static __xdata uint16_t dropped;
static __xdata uint8_t typingId, typingPhase;
static __code uint8_t channels[5] = {2, 6, 5, 4, 3};
static volatile __xdata uint16_t touchISRRaw[5];
static volatile __bit touchReady;
static __xdata uint16_t touchRaw[5], baseline[5];
static __xdata uint32_t baselineSum[5];
static __xdata uint8_t touchIndex, warmup = 10, calibration = 32;
static __xdata uint16_t touchFrames;
static __xdata char tx[192];
static __xdata uint8_t txLen, txPos;
static __xdata uint32_t lastScan, lastReport, lastHid;

void resetKeyboard(void) {
  uint8_t i;
  keyboardBusy = 0;
  hidTail = hidHead;
  typingId = 0;
  for (i = 0; i < 8; i++) Ep3Buffer[i] = 0;
}

static void put(char c) { if (txLen < sizeof(tx)) tx[txLen++] = c; }
static void number(uint32_t n) {
  char digits[10];
  uint8_t count = 0;
  do { digits[count++] = '0' + n % 10; n /= 10; } while (n);
  while (count) put(digits[--count]);
}
static void field(uint32_t n) { put(','); number(n); }

static void enqueue(uint8_t id, uint8_t down) {
  uint8_t next = (eventHead + 1) & 31;
  if (next != eventTail) {
    eventQueue[eventHead] = id | (down ? 0x80 : 0);
    eventHead = next;
  } else dropped++;
  if (down && UsbConfig) {
    next = (hidHead + 1) & 31;
    if (next != hidTail) { hidQueue[hidHead] = id; hidHead = next; }
    else dropped++;
  }
}

static void scanKeys(void) {
  uint8_t ground, low, drive, sense, id = 4;
  uint32_t mask;
  // Open drain + pull-up: high means release, never push-pull against a key.
  P3 |= 0x0F;
  delayMicroseconds(10);
  ground = (~P3) & 0x0F;
  mask = ground;
  for (drive = 0; drive < 4; drive++) {
    P3 &= ~(1 << drive);
    delayMicroseconds(10);
    low = (~P3) & 0x0F & ~ground;
    for (sense = 0; sense < 4; sense++) {
      if (sense == drive) continue;
      if (low & (1 << sense)) mask |= 1UL << id;
      id++;
    }
    P3 |= 0x0F;
    delayMicroseconds(10);
  }
  if (CLOCK_CFG & bRST) mask |= 1UL << 16;
  rawKeys = mask;
  for (id = 0; id < 17; id++) {
    uint32_t bit = 1UL << id;
    if ((mask & bit) == (stableKeys & bit)) debounce[id] = 0;
    else if (++debounce[id] >= 5) {
      debounce[id] = 0;
      stableKeys ^= bit;
      enqueue(id + 1, (stableKeys & bit) != 0);
    }
  }
}

static void keyboardTask(uint32_t now) {
  uint8_t usage = 0, digit, i;
  if (!UsbConfig) { hidTail = hidHead; typingId = 0; return; }
  if (keyboardBusy) return;
  if (!typingId) {
    if (hidTail == hidHead) {
      if (!keyboardIdle || (uint32_t)(now - lastHid) < (uint16_t)keyboardIdle * 4) return;
      typingPhase = 5; // HID idle resend of the released state.
    } else {
      typingId = hidQueue[hidTail];
      hidTail = (hidTail + 1) & 31;
      typingPhase = typingId >= 10 ? 0 : 2;
    }
  }
  // Tens press/release, units press/release, space press/release.
  if (typingPhase == 0) usage = 0x1E;
  else if (typingPhase == 2) {
    digit = typingId % 10;
    usage = digit ? 0x1D + digit : 0x27;
  } else if (typingPhase == 4) usage = 0x2C;
  for (i = 0; i < 8; i++) Ep3Buffer[i] = 0;
  Ep3Buffer[2] = usage;
  UEP3_T_LEN = 8;
  keyboardBusy = 1;
  UEP3_CTRL = (UEP3_CTRL & ~MASK_UEP_T_RES) | UEP_T_RES_ACK;
  lastHid = now;
  if (++typingPhase == 6) typingId = 0;
}

static void calibrate(void) {
  uint8_t i;
  calibration = 32;
  warmup = 10;
  for (i = 0; i < 5; i++) baselineSum[i] = 0;
}

void TouchInterrupt(void) __interrupt(INT_NO_TKEY) {
  touchISRRaw[touchIndex] = TKEY_DAT;
  if (++touchIndex == 5) { touchIndex = 0; touchReady = 1; }
  TKEY_CTRL = bTKC_2MS | channels[touchIndex];
}

static void touchTask(void) {
  uint8_t i;
  if (touchReady) {
    IE_TKEY = 0;
    for (i = 0; i < 5; i++) touchRaw[i] = touchISRRaw[i];
    touchReady = 0;
    IE_TKEY = 1;
    touchFrames++;
    if (warmup) warmup--;
    else if (calibration) {
      for (i = 0; i < 5; i++) baselineSum[i] += touchRaw[i];
      if (--calibration == 0)
        for (i = 0; i < 5; i++) baseline[i] = baselineSum[i] >> 5;
    }
  }
}

static void serialTask(uint32_t now) {
  uint8_t i, event;
  while (USBSerial_available()) {
    char c = USBSerial_read();
    if (c == 'c' || c == 'C') calibrate();
  }
  if (!(controlLineState & 1) || !UsbConfig) {
    txLen = txPos = 0;
    eventTail = eventHead;
    return;
  }
  // Never wait for a USB transfer in the scanning loop.
  if (UpPoint2BusyFlag) return;
  if (txPos < txLen) {
    while (txPos < txLen && usbWritePointer < 63) USBSerial_write(tx[txPos++]);
    USBSerial_flush();
    return;
  }
  txPos = txLen = 0;
  if (eventTail != eventHead) {
    event = eventQueue[eventTail];
    eventTail = (eventTail + 1) & 31;
    put('K'); field(now); field(event & 0x7F); field(event >> 7); put('\n');
  } else if ((uint32_t)(now - lastReport) >= 50) {
    lastReport = now;
    put('T'); field(now); field(touchFrames); field(stableKeys); field(rawKeys);
    field(warmup || calibration); field(dropped);
    for (i = 0; i < 5; i++) field(touchRaw[i]);
    for (i = 0; i < 5; i++) field(baseline[i]);
    put('\n');
  }
}

void main(void) {
  init();
  P3 |= 0x0F;
  P3_MOD_OC |= 0x0F;
  P3_DIR_PU |= 0x0F;
  P1 |= 0xF2;
  P1_DIR_PU &= ~0xF2;
  P1_MOD_OC &= ~0xF2;
  TKEY_CTRL = bTKC_2MS | channels[0];
  IE_TKEY = 1;
  USBInit();
  for (;;) {
    uint32_t now = millis();
    touchTask();
    if ((uint32_t)(now - lastScan) >= 1) { lastScan = now; scanKeys(); }
    keyboardTask(now);
    serialTask(now);
  }
}

unsigned char __sdcc_external_startup(void) __nonbanked { return 0; }
