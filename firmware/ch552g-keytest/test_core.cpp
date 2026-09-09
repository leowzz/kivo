// Host-side behavioral checks of main.c, with an electrical scan-network mock.
#include <cassert>
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <string>

#define __xdata
#define __idata
#define __data
#define __code const
#define __bit bool
#define __at(x)
#define __interrupt(x)
#define __using(x)
#define __nonbanked
#define bRST 8
#define bTKC_IF 128
#define bTKC_2MS 16
#define MASK_UEP_T_RES 3
#define UEP_T_RES_NAK 2
#define UEP_T_RES_ACK 0

static uint32_t physicalKeys;
struct Port {
    uint8_t latch = 255;
    Port& operator|=(int n) { latch |= n; return *this; }
    Port& operator&=(int n) { latch &= n; return *this; }
    operator int() const {
        uint8_t low = (~latch & 15) | (physicalKeys & 15);
        // A pressed diode key propagates a driven low to its sense line.
        for (int pass = 0; pass < 4; pass++) {
            int id = 4;
            for (int drive = 0; drive < 4; drive++)
                for (int sense = 0; sense < 4; sense++) {
                    if (drive == sense) continue;
                    if ((physicalKeys & (1UL << id)) && (low & (1 << drive)))
                        low |= 1 << sense;
                    id++;
                }
        }
        return (latch & 240) | (~low & 15);
    }
} P3;
uint8_t UEP3_T_LEN, UEP3_CTRL, CLOCK_CFG, TKEY_CTRL, P3_MOD_OC, P3_DIR_PU;
uint8_t P1, P1_DIR_PU, P1_MOD_OC, IE_TKEY;
uint16_t TKEY_DAT;
volatile bool UpPoint2BusyFlag;
volatile uint8_t controlLineState, UsbConfig;
uint8_t usbWritePointer, keyboardIdle, Ep3Buffer[64];
void init() {}
void USBInterrupt() {}
void USBInit() {}
void delayMicroseconds(uint16_t) {}
uint32_t millis() { return 0; }
uint8_t USBSerial_available() { return 0; }
char USBSerial_read() { return 0; }
void USBSerial_write(char) { usbWritePointer++; }
void USBSerial_flush() { UpPoint2BusyFlag = true; usbWritePointer = 0; }

#define main firmware_main
#include "main-host.inc"
#undef main

int main() {
    UsbConfig = 1;
    for (int key = 1; key <= 17; key++) {
        stableKeys = rawKeys = 0;
        memset(debounce, 0, sizeof(debounce));
        hidHead = hidTail = eventHead = eventTail = 0;
        typingId = 0;
        keyboardBusy = false;
        physicalKeys = key == 17 ? 0 : 1UL << (key - 1);
        CLOCK_CFG = key == 17 ? bRST : 0;
        for (int n = 0; n < 4; n++) scanKeys();
        assert(stableKeys == 0); // Reject a short pulse before debounce settles.
        scanKeys();
        assert(stableKeys == (1UL << (key - 1)));
        assert(eventHead == 1 && eventQueue[0] == (key | 128));
        std::string typed;
        for (int n = 0; n < 100; n++) {
            scanKeys();
            keyboardTask(n);
            if (keyboardBusy) {
                uint8_t code = Ep3Buffer[2];
                if (code == 0x2C) typed += ' ';
                else if (code == 0x27) typed += '0';
                else if (code) typed += char('1' + code - 0x1E);
                USB_EP3_IN();
            }
        }
        assert(typed == std::to_string(key) + " "); // Also covers ID 11 repeated digit.
        assert(eventHead == 1); // A held key must not repeat.
        physicalKeys = CLOCK_CFG = 0;
        for (int n = 0; n < 5; n++) scanKeys();
        assert(stableKeys == 0 && eventHead == 2 && eventQueue[1] == key);
        assert((P3.latch & 15) == 15);
        assert((P3.latch & 240) == 240); // Other port pins preserved.
    }
    calibrate();
    touchIndex = 0;
    for (int frame = 0; frame < 42; frame++) {
        for (int i = 0; i < 5; i++) {
            TKEY_CTRL = bTKC_IF;
            TKEY_DAT = 5000 + i * 100;
            TouchInterrupt();
            touchTask();
            assert(TKEY_CTRL == (bTKC_2MS | channels[(i + 1) % 5]));
        }
    }
    assert(!calibration && !warmup);
    for (int i = 0; i < 5; i++) assert(baseline[i] == 5000 + i * 100);
    for (int n = 0; n < 1000; n++) {
        TKEY_CTRL = bTKC_IF; TKEY_DAT = 4000; TouchInterrupt(); touchTask();
    }
    assert(baseline[0] == 5000 && touchRaw[0] == 4000); // Long touch stays visible.
    controlLineState = 1; txPos = 0; txLen = 20; UpPoint2BusyFlag = true;
    serialTask(1000);
    assert(txPos == 0); // A blocked host never stalls scanning.
    puts("PASS: 17 scan IDs, debounce, hold/release, digit sequences, touch calibration, CDC backpressure");
}
