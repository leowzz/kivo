# Kivo Workbench r03 PCB Draft

This is an editable electrical and placement draft, **not a board ready to
order**. Open `workbench-r03.kicad_pro` in KiCad 10. The PCB is intentionally
unrouted, and no Gerber, drill, or assembly-order files are provided.

## Confirmed Scope

- RP2040 chip directly on the main PCB, with factory assembly for the QFN.
- 18 ordinary MX switches with Kailh-style hot-swap sockets on the PCB back.
- USB-C for USB 2.0 device data and 5 V power.
- Existing SH1106, EC11, encoder press, confirm and back module connected by a
  harness. The module remains a separate panel-mounted assembly.
- Enclosure changes and reprinting are permitted.

The existing `r02` product YAML remains the source of the GPIO mapping. `r03`
here is a proposed new hardware revision, not a released Product Version ID.

## Files

| File | Purpose |
| --- | --- |
| `workbench-r03.kicad_sch` | Embedded-symbol schematic, 59 components |
| `workbench-r03.kicad_pcb` | Four-layer, 1.6 mm placement draft with 18 bottom sockets |
| `workbench-r03.kicad_pro` | KiCad project and preliminary design rules |
| `Workbench.pretty/` | Local hot-swap socket and measured-head mounting footprints |
| `key-coordinates.csv` | Model panel coordinates and PCB coordinates, in mm |
| `placement.json` | Reproducible placement and pin-to-net manifest |
| `bom-draft.csv` | Uncosted component list; not a factory purchasing BOM |
| `verification.md` | Checks performed and open issues |

## Electrical Design

The RP2040 core follows the Raspberry Pi hardware design guide: 3.3 V I/O,
the internal 1.1 V regulator feeding DVDD, TESTEN tied to ground, local bypass
capacitors, QSPI flash, and an external 12 MHz crystal. The proposed parts are:

| Function | Proposed part / connection |
| --- | --- |
| MCU | RP2040, QFN-56 with exposed ground pad |
| Flash | W25Q128JVSIQ, 128 Mbit / 16 MiB, 208 mil SOIC-8 |
| Crystal | Abracon ABM8-272-T3, 12 MHz, 15 pF loads, 1 kohm damping resistor |
| 3.3 V regulator | AP2112K-3.3TRG1, 1 uF input and 4.7 uF output |
| USB-C | GCT USB4105-GF-A, USB 2.0, separate 5.1 kohm CC1/CC2 pull-downs |
| USB data | USBLC6-2SC6 protection, 27 ohm series resistors near RP2040 |
| BOOTSEL | Push switch through 1 kohm to QSPI_SS; 10 kohm pull-up fitted |
| RESET | Push switch to RUN, with 10 kohm pull-up |
| Keys | Each GPIO to one socket contact; the other contact to GND |
| I2C pull-ups | R9/R10 4.7 kohm footprints, DNP pending module inspection |

Direct GPIO inputs use the existing firmware's pull-ups and debounce. This
topology does not need matrix diodes. The 16 MiB flash matches the current
YD-RP2040 build's maximum flash size. That does not, by itself, validate the
custom board's firmware identity or boot behavior.

These are proposed MPNs, not a stock-checked shopping list. In particular,
confirm the purchased socket's drawing against the vendored footprint, and
confirm the exact USB connector suffix, plating and packaging before ordering.
The PCB includes no microphone, speaker, battery, charger or RGB lighting.

## Main Board Interfaces

J2 is a **main-board harness pinout chosen for this PCB**. It is not evidence of
the left-to-right pin order of the purchased display module. Use individually
mapped wires and the module's labels; do not assume a straight-through cable.
The repository's YAML provides the GPIO assignments below, but does not record
the module connector numbering or power specification.

| J2 Pin | Main Board Signal | RP2040 GPIO |
| --- | --- | --- |
| 1 (square pad) | GND | - |
| 2 | 3.3 V | - |
| 3 | SCL | 27 |
| 4 | SDA | 26 |
| 5 | Confirm / OK | 22 |
| 6 | Encoder press | 28 |
| 7 | Encoder A | 21 |
| 8 | Encoder B | 20 |
| 9 | Back | 19 |

The main-board header is a 1x9, 2.54 mm pitch header. It is placed vertically
in the PCB top view with pin 1 closest to the USB edge. J2 provides **3.3 V only**;
verify the module operates at 3.3 V and that its signals and I2C pull-ups do not
drive 5 V into the RP2040. R9/R10 should be fitted only if the module lacks
suitable pull-ups. The guide's "seven GPIO" count excludes power and ground.

J3 pins 1-4 are 3.3 V reference, GND, SWDIO and SWCLK. A debugger should sense
the target voltage without powering a simultaneously USB-powered board.

J4 exposes all five unused RP2040 GPIOs at the **rear-left (upper-left in top
view)** of the PCB. It is a 1x7 plated through-hole header, 2.54 mm pitch with
1.0 mm drills. Viewed from the component side with USB at the top, pin 1 is
the leftmost square pad; the pins run left to right:

| J4 Pin | Signal |
| --- | --- |
| 1 (square pad) | GND |
| 2 | 3.3 V |
| 3 | GPIO0 |
| 4 | GPIO23 |
| 5 | GPIO24 |
| 6 | GPIO25 |
| 7 | GPIO29 / ADC3 |

These pins connect directly to the RP2040 and use **3.3 V logic, not 5 V**.
External loads must fit within the regulator's remaining current and thermal
budget. GPIO24/25 are free on this bare-chip design, but the current YD-RP2040
firmware pin whitelist excludes them. Enable them in the future custom-board
definition before use; this hardware change does not change the product
profile or assign functions to any expansion pins.

J5 (BOOT) and J6 (RESET) sit immediately to the left of the rear USB-C port.
Each is a 1x2 plated through-hole footprint, 2.54 mm pitch with 1.0 mm drills,
for a matching normally-open momentary button or short wires to a button.
They are not a universal four-leg tactile-switch footprint. Viewed from the
component side with USB at the top, pin 1 is the left square pad:

| Header | Pin 1 | Pin 2 | Connection |
| --- | --- | --- | --- |
| J5 BOOT | BOOT_BUTTON | GND | Parallel to SW19; through R7 (1 kohm) to QSPI_SS |
| J6 RESET | RUN | GND | Parallel to SW20; R8 (10 kohm) pulls RUN up to 3.3 V |

The onboard SW19/SW20 buttons remain fitted. To enter the USB bootloader,
hold BOOT, press and release RESET, then release BOOT. Do not short QSPI_SS
directly to ground; J5 preserves the existing series resistor.

K1-K18 retain GPIO1-GPIO18. Viewed from the key side with USB at the back,
K1-K6 are the row closest to the display, K7-K12 are the middle row, and K13-K18
are closest to the user. The geometry script does not assign key IDs; this
numbering follows the current product layout's reading order.

## Mechanical Basis

Source: `scripts/modeling/integrated_workstation.py`, specifically
`KEY_COLUMNS`, `KEY_ROWS`, `KEY_PITCH`, `KEY_X0`, `KEY_Y0`, `PANEL_X0`, and
`PANEL_Y0`. The generated switch centers use the **flat panel plane**, not a
top-down projection of its 30-degree assembled position.

- Existing panel: 132 x 117 mm, tilted 30 degrees.
- Proposed board: 126 x 105 mm, 1.6 mm thick, parallel to the panel.
- Model panel coordinates: X to the right, Y from the front towards the rear.
- Board coordinates in CSV: origin at the board's rear-left corner, X right,
  Y towards the user. Conversion: `pcb_x = panel_x - 3`,
  `pcb_y = 113 - panel_y`.
- The KiCad page uses an additional (50, 50) mm translation for readability.
- 19.05 mm switch pitch is preserved. The five 3.4 mm PCB holes are **new**
  proposed mounting positions; they do not reuse the existing panel screw axes.
- The local mounting footprint assumes a screw head no larger than 5.6 mm,
  with a 6.0 mm clearance envelope and no large washer. This follows the
  project's measured 5.3 mm head, but actual fasteners and bosses need checking.

The current shell has two solid support walls in the key field, six fly-wire
clips, a bottom-mounted development-board cradle and low rear USB openings.
These features require redesign around the new board. The existing two-switch
pod also needs a decision during the enclosure revision because it is absent
from the current product's electrical configuration.

The `Dwgs.User` display rectangle is a reserved area, not a validated 3D module
model. Before final PCB dimensions, verify the switch plate-to-PCB distance,
socket underside height, display connector height, panel screw access and USB
plug insertion. Include the J4 header/mating connector height near the display
and the J5/J6 button or wire clearance in the enclosure check. The new USB port
follows the sloped board and cannot align with
the old bottom-board USB opening. The existing eight-pin modeling constant is
inconsistent with the apparent nine contacts in `refer/dsp-encp.png`; keep the
electrical harness independent of that mechanical assumption.

## Remaining Design Work

1. Verify the actual display module's voltage, connector labels and pull-ups,
   and the exact socket and USB connector drawings.
2. Establish a manufacturer stackup and impedance geometry. The four-layer
   board is proposed to give USB and the RP2040 a continuous reference plane;
   the current layer count is not a validated impedance specification.
3. Complete placement for short crystal, QSPI, regulator and decoupling paths,
   then route, pour ground planes and rerun DRC. Keep the USB pair over continuous
   ground and determine widths/gap for 90 ohm differential impedance from the
   selected stackup. Review QFN ground and paste/thermal-via assembly details.
4. Revise and collision-check the enclosure in 3D, including MX plate capture
   thickness, new PCB mounts and the USB opening. Existing STLs are unchanged.
5. Add the new board identity/build target and an r03 product definition when
   the electrical design is frozen; validate USB descriptors, flash size,
   BOOTSEL, reset, HID/CDC, all 18 keys, display and controls on assembled boards.
6. Select purchasable parts, complete ERC/DRC and assembly review, and then
   export Gerbers, drill files, BOM and placement files for the board house.

## Reproduction

The checked-in KiCad files are the editable design. The generators are a way to
reproduce this first draft, not a substitute for the PCB editor. Generate into a
new directory to preserve manual changes:

```sh
uv run --script scripts/hardware/generate_workbench.py --output /tmp/workbench-review
```

Copy `Workbench.pretty`, `3dmodels`, `fp-lib-table` and the project settings from
this directory into that new directory, then run the placement script using
KiCad's bundled Python interpreter:

```sh
/Applications/KiCad/KiCad.app/Contents/Frameworks/Python.framework/Versions/3.9/bin/python3.9 \
  scripts/hardware/place_workbench.py /tmp/workbench-review/placement.json \
  /tmp/workbench-review/workbench-r03.kicad_pcb
```

The placement script refuses to overwrite an existing PCB. Both scripts accept
explicit library paths for KiCad installations at other locations.

## Sources

- Existing product: `products/kivo-workbench-rp-k18-disp-encp-r02/product.yaml`.
- [Raspberry Pi Hardware Design with RP2040](https://datasheets.raspberrypi.com/rp2040/hardware-design-with-rp2040.pdf), release 2,
  sections 2.1-2.4. The source reference design's routing was not transplanted.
- [foostan/kbd MX hot-swap footprint](https://github.com/foostan/kbd/blob/main/kicad-footprints/kbd.pretty/keyswitch_cherrymx_hotswap_1u.kicad_mod),
  retrieved 2026-09-05, upstream blob `3601d4e1d8f8c5b49b9d56c87e00ec56cd4921a1`.
  Only its 3D-model path was changed. The matching socket STEP model is vendored
  from the same repository. License: `licenses/foostan-kbd-MIT.txt`.
- Standard electronic symbols, packages and 3D models come from installed
  KiCad 10.0.6 libraries. The schematic embeds the symbols for portability.
