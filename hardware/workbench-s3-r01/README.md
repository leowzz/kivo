# Workbench S3 r01 - Socketed Carrier

Independent **unrouted review draft**, using the YD ESP32-S3 core board from
`refer/YD-ESP32-S3/`. Open `workbench-s3-r01.kicad_pro` in KiCad 10.
The RP2040 design remains in `../workbench-r03/`. This revision is not ready
for fabrication and is not a released product definition.

## Architecture

- Preassembled YD ESP32-S3 plugs into two 1x22 female sockets, J1 and J7.
- Reuse the core board's native USB Type-C for HID/data and power.
- 18 MX hot-swap keys use a 3x6 matrix with one 1N4148W diode per key.
- SH1106/EC11/confirm/back module connects by the J2 harness.
- J4 exposes eight general-purpose spare GPIOs, GND and 3.3 V.
- J5/J6 provide external BOOT/RESET button connections.
- Main-board parts are through-hole connectors, SOD-123 diodes, and 0805
  resistors/capacitors with extended hand-solder pads.
- No bare MCU, flash, crystal, regulator or fine-pitch USB connector needs
  soldering on this carrier.

## Core Board Sockets

The manufacturer's metric drawing specifies 27.94 mm width, 57.15 mm PCB
length, 63.39 mm including the antenna, 25.40 mm between header rows and
53.34 mm between the first and last holes. Both rows have 22 pins at 2.54 mm
pitch; end holes are approximately 1.91 mm from each PCB end.

Viewed from the carrier's component side with USB at the rear/top:

| Carrier | Core Board Row | Position | Pin 1 |
| --- | --- | --- | --- |
| J1 | Manufacturer P1 | Right | 3V3, nearest antenna/front |
| J7 | Manufacturer P2 | Left | GND, nearest antenna/front |

Both socket rows run from pin 1 towards pin 22 at the USB end. The core board
is rotated 180 degrees from the manufacturer's antenna-up illustration.
Use 2.54 mm female sockets with 1.0 mm PCB drills. The two row centers are
25.40 mm apart; do not substitute a narrower ESP32 board footprint.

The full pin mapping, including deliberately unconnected pins, is in
`placement.json`. GPIO19/20 stay exclusively on the module's USB connection.
GPIO35-37 are not used, allowing modules with octal PSRAM. GPIO43/44 remain
with the serial bridge, GPIO48 with the module RGB LED, and GPIO3/45/46 are
not assigned external loads because of boot-strapping constraints.

Power the assembly through the core board. The carrier uses its 3.3 V and GND.
P1 pin 21 is intentionally unconnected: the reference board's 5V pin is on
the input side of diode D3 unless its IN-OUT jumper is fitted. It must not
be assumed to provide USB-derived 5 V. External GPIO and I2C signals are
3.3 V only, and peripheral loads must fit the module regulator's remaining
current and thermal budget.

## Matrix

| Function | GPIOs |
| --- | --- |
| Rows R0-R2, driven low one at a time | 4, 5, 6 |
| Columns C0-C5, read with pull-ups | 7, 8, 9, 10, 11, 12 |

K1-K6 are the rear key row, K7-K12 the middle, and K13-K18 the front.
For every key: column -> switch -> diode anode A -> diode cathode K -> row.
**The stripe on each diode goes to the row net.** D1 belongs to K1, and so on.
Use 1N4148W in SOD-123, not the smaller SOD-323 version.

This reduces key GPIO use from 18 to 9. The display/control module uses 7
additional GPIOs. Diodes prevent electrical ghost paths; they do not by
themselves change the firmware's multi-key filtering behavior.

## Other Interfaces

All pin orders below are from the component side with USB at the top.

| J2 Pin | Signal | GPIO |
| --- | --- | --- |
| 1, square pad at rear | GND | - |
| 2 | 3.3 V | - |
| 3 | SCL | 14 |
| 4 | SDA | 13 |
| 5 | Confirm | 15 |
| 6 | Encoder press | 16 |
| 7 | Encoder A | 17 |
| 8 | Encoder B | 18 |
| 9 | Back | 21 |

J2 is the carrier's chosen 1x9 harness order, not a verified pin-for-pin
module connector order. Check the actual module's power rating and labels.
R1/R2 are optional 4.7 kohm I2C pull-ups: fit them only if the module does
not already provide suitable pull-ups.

J4 sits at the upper-left edge. Its 1x10, 2.54 mm pitch plated holes have
1.0 mm drills. From the left square pad to the right:

| Pin | 1 | 2 | 3 | 4 | 5 | 6 | 7 | 8 | 9 | 10 |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| Signal | GND | 3V3 | GP1 | GP2 | GP38 | GP39 | GP40 | GP41 | GP42 | GP47 |

J5/J6 sit next to the core board's USB end. Each has two 2.54 mm pitch,
1.0 mm drilled plated holes for a normally-open momentary button with matching
leads, or short wires to a button. These are not universal four-leg button
footprints.

| Header | Left Square Pad, Pin 1 | Right Pad, Pin 2 |
| --- | --- | --- |
| J5 BOOT | GPIO0 | GND |
| J6 RESET | EN / CHIP_PU | GND |

They parallel the core board buttons. R3 adds a 10 kohm BOOT pull-up; the
module already supplies the EN pull-up and reset capacitors. Hold BOOT,
press/release RESET, then release BOOT to request the ROM download mode.

## Mechanical Basis

The proposed carrier is 126 x 135 mm, two copper layers, 1.6 mm thick.
Its rear is 30 mm deeper than the RP2040 carrier. The 18 key centers retain
19.05 mm pitch and their relative geometry; only their distance from the
new rear edge changes. A new enclosure and panel are allowed and required.
Existing STL and CAD outputs have not been regenerated.

The core board occupies the right rear, USB facing outward. An opening under
its antenna and a two-layer no-copper/no-component rule area reserve RF
clearance. Keep metal, cables and enclosure screws away from that area.
This is a placement provision, not measured RF performance.

Choose female socket height only after checking the actual core-board
underside and pin length. Include socket height, unplugging access, both USB
plugs, external buttons, the display harness and MX plate capture in the new
enclosure. The display rectangle and core-board outline are drawing references,
not validated 3D assemblies. Rendered previews show the carrier and sockets,
without a fitted core-board model.

## Hand Assembly

1. Solder the 0805 parts and 18 SOD-123 diodes first. Check every diode stripe.
2. Solder Kailh sockets on the back, keeping the iron and solder off plastic.
   Support each socket flat on the PCB; inspect both contacts for wetting.
3. Fit the two 1x22 female sockets using the unpowered core board as an
   alignment jig. Tack one end of each row, check seating, then finish.
4. Fit the display/expansion headers and external buttons or wires.
5. Inspect bridges and polarity and measure supply-to-ground resistance
   before plugging in the core board or applying power.

Buy one preassembled YD ESP32-S3 core board and two matching 1x22 male strips
if the core board is supplied without pins. `bom-draft.csv` lists the carrier
parts; the purchased core board, switch bodies, keycaps and display module
are separate assemblies. Ordinary soldering-iron assembly is intended.

## Firmware Status

`firmware-profile-draft.yaml` records the new wiring for future integration;
it is deliberately outside `products/`. The current r02 product uses RP2040
and direct keys and cannot be used unchanged.

- `src/platform/esp32s3.cpp` currently has no display implementation, and
  `kYdEsp32S3.supportsOled` is false. SH1106 support must be implemented.
- Existing matrix scanning drives rows low and reads pulled-up columns,
  matching the diode direction here. The firmware's contact-cycle filter
  still suppresses some multi-key combinations; add explicit diode-matrix
  support before claiming full rollover.
- GP39-GP42 are usable core-board pins but excluded from the present firmware
  whitelist. Enable them when assigning expansion functions.
- Verify the actual flash/PSRAM variant, build target and USB behavior on
  assembled hardware before releasing an S3 product definition.

## Files And Reproduction

Schematic, PCB, project, BOM, placement manifest and key-coordinate CSV are
editable sources. `verification.md` records checks and remaining work.
Generators refuse to overwrite existing designs:

```sh
uv run --script scripts/hardware/generate_workbench_s3.py --output /tmp/workbench-s3-review
```

Copy this directory's `Workbench.pretty`, `3dmodels`, `licenses`,
`fp-lib-table` and project file into that new directory, then run:

```sh
/Applications/KiCad/KiCad.app/Contents/Frameworks/Python.framework/Versions/3.9/bin/python3.9 \
  scripts/hardware/place_workbench_s3.py /tmp/workbench-s3-review/placement.json \
  /tmp/workbench-s3-review/workbench-s3-r01.kicad_pcb
```

The S3 schematic generator reuses symbol-format helpers from
`generate_workbench.py`; the RP2040 design scripts are otherwise unchanged.

## Sources

- `refer/YD-ESP32-S3/README.md`, manufacturer P1/P2 pin tables.
- `refer/YD-ESP32-S3/5-public-YD-ESP32-S3-Hardware info/ESP32-S3-Metric.pdf`.
- `refer/YD-ESP32-S3/5-public-YD-ESP32-S3-Hardware info/YD-ESP32-S3-SCH-V1.4.pdf`.
- Original key geometry: `scripts/modeling/integrated_workstation.py`.
- Kailh socket footprint/model from foostan/kbd, copied from the RP2040
  draft with its MIT license in `licenses/`.
- Standard socket, diode and hand-solder footprints: KiCad 10.0.6 libraries.
