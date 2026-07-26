# LuatOS ESP32S3-AIO Dual LED Toggle Design

## Goal

Run a repeating 3 Hz blink on one onboard LED while the other onboard LED
stays on. Each distinct low pulse on GPIO6 swaps the two LED roles.

## Hardware Mapping

- LEDA: GPIO10, active high
- LEDB: GPIO11, active high
- Mode input: GPIO6, configured with the ESP32-S3 internal pull-up
- The external mode control connects GPIO6 to ground when active

The LED mapping and polarity follow the official LuatOS ESP32S3-CORE board
documentation. The connected board exposes the expected CH343 USB serial
bridge as `/dev/cu.usbmodem575E0212961`.

## Startup State

- LEDA blinks at 3 Hz.
- LEDB stays on.
- GPIO6 starts armed for a low-level activation.

## Blink Timing

One complete flash consists of one on interval and one off interval. The
period is one third of a second, with a 50 percent duty cycle:

- On time: approximately 166,667 microseconds
- Off time: approximately 166,667 microseconds
- Complete flashes: 3 per second

Timing is non-blocking so GPIO6 remains responsive while an LED is blinking.
Small scheduler and integer rounding jitter is acceptable, but timing must not
accumulate drift by resetting the schedule from the current time on every
normal tick.

## Mode Switching

GPIO6 is sampled continuously and debounced for 30 milliseconds.

1. A stable transition from high to low swaps the LED roles once.
2. The previously blinking LED becomes steadily on immediately.
3. The previously steady LED begins a new blink cycle immediately in its on
   phase.
4. Holding GPIO6 low does not cause additional swaps.
5. GPIO6 must return to a stable high level before another low activation can
   swap the roles again.

## Software Structure

Use PlatformIO with the Arduino framework and an ESP32-S3 target. Keep the
hardware-independent state transition and timing decisions in a small module
that accepts sampled input levels and elapsed time. The Arduino entry point
owns pin configuration, reads GPIO6, applies the resulting LED levels, and
supplies a monotonic microsecond timestamp.

This separation allows host-side tests to verify frequency, startup state,
debouncing, held-low behavior, and repeated role switching without requiring
the physical board for every test run.

## Verification

- Host tests prove the initial outputs and 3 Hz timing decisions.
- Host tests prove that input bounce and a held-low signal cause one swap only.
- Host tests prove that a stable high release re-arms the next swap.
- PlatformIO builds the firmware for ESP32-S3.
- The firmware is uploaded through `/dev/cu.usbmodem575E0212961`.
- A final physical check confirms one LED blinks three times per second, the
  other stays lit, and grounding GPIO6 repeatedly swaps their roles.
