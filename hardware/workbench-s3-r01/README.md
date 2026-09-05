# Workbench S3 r01 - Stacked Breakaway Boards

Independent **unrouted review draft**, using the YD ESP32-S3 core board from
`refer/YD-ESP32-S3/`. Open `workbench-s3-r01.kicad_pro` in KiCad 10.
The RP2040 design remains in `../workbench-r03/`. This revision is not ready
for fabrication and is not a released product definition.

## Architecture

- Manufacture a single mechanically connected 126 x 187 mm panel, then cut
  the perforated tabs into an upper key board and a lower controller board.
- Assemble the 126 x 86 mm lower board horizontally and the 126 x 98 mm upper
  board rear-high at a nominal 30 degrees. The PCB projection fits in 86 mm
  depth, compared with 135 mm for the previous single-board draft.
- J8 on the lower board and J9 on the upper underside connect by one IDC20
  ribbon cable. There are no electrical connections through the tabs.
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

All pin orders below use each separated board's front/component-side view,
rear edge at the top. The lower board is rotated 180 degrees in the panel.

| J2 Pin | Signal | GPIO |
| --- | --- | --- |
| 1, rightmost square pad | VCC / 3.3 V | - |
| 2 | GND | - |
| 3 | BAK / KEY0 / Back | 21 |
| 4 | TRB / TRIM_B / Encoder B | 18 |
| 5 | TRA / TRIM_A / Encoder A | 17 |
| 6 | PSH / PUSH / Encoder press | 16 |
| 7 | SCL / IIC_SCL | 14 |
| 8 | SDA / IIC_SDA | 13 |
| 9, leftmost pad | CON / KEY1 / Confirm | 15 |

J2 matches the supplied module interface diagram and front photograph in
`references/display-interface.png`. Looking at the readable display with
the encoder on the right, the top connector reads **CON, SDA, SCL, PSH,
TRA, TRB, BAK, GND, VCC from left to right**. This is pin 9 through pin 1.
VCC is **3.3 V only**. Use a pin-for-pin 9-wire harness and identify the
square pad rather than inferring pin numbers from a rear or mating-face view.
The GPIO assignments to display functions are unchanged; J8/J9 also retain
their existing cable mapping.

The dimension source is `references/display-dimensions.png`. The module's
64.90 x 35.03 mm bounding rectangle is placed at upper-board `(8,3)` mm,
front-facing with the encoder on the right. `Dwgs.User` shows this assembly
envelope, including the notched board's full rectangular extent; it is not
a cutout in the carrier. The module connector runs horizontally near the
rear edge. Its leftmost pin is 11.38 mm from the module's left edge and
the row is 1.93 mm below its top edge. On the carrier, J2 pin 9 is at
`(19.38,4.93)` and pin 1 is at `(39.70,4.93)` mm.

The 2.54 mm header pitch is the standard pitch assumed for this module;
it is not explicitly dimensioned in the supplied drawing. Pin 1's X value
uses `11.38 + 8 * 2.54 = 31.70` mm. Confirm pitch against the purchased
module before fabrication. Carrier J2 uses 1.0 mm plated drills.

Four additional 3.4 mm NPTH clearance holes follow the module's M3 mounting
centers. The drawing's slight asymmetry is retained:

| Mount | Relative To Module Top-Left | Relative To Upper PCB Top-Left |
| --- | --- | --- |
| H_D1, rear left | (2.87, 2.85) | (10.87, 5.85) |
| H_D2, rear right | (61.90, 2.90) | (69.90, 5.90) |
| H_D3, front left | (2.95, 32.06) | (10.95, 35.06) |
| H_D4, front right | (61.93, 31.88) | (69.93, 34.88) |

These module mounts are separate from the upper PCB's four enclosure mounts.
Set module standoff height and header/harness direction from the actual
assembly; this connector is still a harness interface, not a validated
rigid board-to-board mating stack. Keep underside screw heads within the
3.5 mm nominal parts envelope or recheck the stack clearances.

R1/R2 are optional 4.7 kohm I2C pull-ups: fit them only if the module does
not already provide suitable pull-ups.

J4 sits at the lower board's rear-left edge. Its 1x10, 2.54 mm pitch plated holes have
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

## Board-to-Board Cable

Use two shrouded, keyed **2x10, 2.54 mm pitch, vertical through-hole IDC
headers** and two matching female IDC cable connectors. The ribbon itself
has **20 conductors at 1.27 mm pitch**. Start with a 150 mm cable, including
service slack, and confirm its exit direction, bend radius and strain relief
against the actual connectors before finalizing the enclosure.

J8 mounts on the lower board's top; J9 mounts on the upper board's bottom.
They face into the space between boards. The connection is **pin 1 to pin 1,
through pin 20 to pin 20**, regardless of how the ribbon is folded. Use the
square pad, connector triangle and red conductor to identify pin 1. Do not
infer numbering by looking at the cable mating face; check continuity before
powering the assembly. Reversed or offset insertion can connect power to GPIO.

| Pins | Signals In Pin Order |
| --- | --- |
| 1-5 | GND, 3V3, GPIO4, GPIO5, GPIO6 |
| 6-10 | GPIO7, GPIO8, GPIO9, GPIO10, GPIO11 |
| 11-15 | GPIO12, GND, GPIO13, GPIO14, GPIO15 |
| 16-20 | GPIO16, GPIO17, GPIO18, GPIO21, 3V3 |

`interconnect.csv` is the complete pin-by-pin mapping. Upper-board schematic
nets use `UP_` prefixes, including `UP_GND` and `UP_3V3`; lower-board nets use
the original names. The external cable joins these separate circuit domains.
Never merge the names or route across the breakaway tabs to remove airwires.
The duplicate power and ground conductors are parallel paths, not separate
power supplies. Start I2C testing at 100 kHz; verify pull-ups and signal
integrity with the actual display cable and module.

## Panel And Separation

Both boards use two copper layers and 1.6 mm FR-4. Panel coordinates below
start at the top-left of the outline; KiCad drawing coordinates add 50 mm
to X and Y.

| Section | Panel Coordinates | Finished Size |
| --- | --- | --- |
| Upper | X 0-126, Y 0-98 | 126 x 98 mm |
| Routed separation channel | Y 98-101 | 3 mm wide |
| Lower, rotated 180 degrees | X 0-126, Y 101-187 | 126 x 86 mm |

The lower-board transform is `(panel_x, panel_y) = (126-x, 187-y)`.
Rotate it back 180 degrees in-plane after separation. Its sockets remain
on the top side, with Type-C at the rear. Do not turn the lower board over.

Two internal racetrack slots and open side notches leave three nominal
5 mm wide tab necks centered at X 20, 63 and 106 mm. Each tab has two rows of
six **0.5 mm NPTH holes at 0.8 mm pitch**, at Y 98.75 and 100.25: 36 holes
total. `Edge.Cuts` defines the actual routed slots; the holes are drilling
features. This is tab routing with mouse bites, not a V-score line.

Cut both perforated rows with flush cutters while supporting each board,
then file the remaining tab stubs back to Y 98 and Y 101. The outer holes
leave roughly 1.25 mm of stub at each finished edge; the two rectangular
separated-board previews assume these stubs have been removed. Separate and
deburr the bare panel before soldering. Do not bend the panel with the
core board, display or other parts installed.

The Y 96-103 band forbids copper tracks, vias and pours on both layers.
Only mechanical holes are permitted there; the verifier also checks that
electrical pads stay out. Each finished board has its own four M3 enclosure
mounting holes, in addition to the upper board's four display mounts, so the
tabs carry no load in the finished assembly.

Confirm the slot tooling, perforation drill/spacing, tab strength and
customer-panelization policy with the chosen fabricator before ordering.
Two circuits connected by tabs may still be charged as two designs; the
126 x 187 mm panel area is larger than the previous single board. A shorter
enclosure does not imply cheaper PCB fabrication or lower print volume.

## Stacked Assembly

Nominal dimensions are in `placement.json` under `stack`; the generated
`stack-side.png` and `stack-clearances.json` show an envelope study. They are
not a completed enclosure or a fitted model of the purchased parts.

- Lower PCB top is Z = 0; its rear edge is Y = 0, with USB facing outward.
- Upper PCB rear is also Y = 0. Tilt it 30 degrees, rear high/front low.
  Its front underside is Z = 25 mm and rear underside is Z = 74 mm.
- Including 1.6 mm board thickness, upper-board depth projects to 85.67 mm.
  The two boards therefore fit a nominal 126 x 86 mm PCB footprint.
- Allow a maximum core-board/socket envelope of Z = 20 mm, antenna top
  Z = 14.5 mm, upper underside parts of 3.5 mm, and mated IDC envelopes
  of 18 mm. Under these assumptions, core-to-upper-parts clearance is
  14.69 mm normal to the upper plane; antenna clearance is 16.33 mm.
- The upper IDC envelope clears the core envelope vertically by 29.07 mm.
  J8 is beside the core, not underneath it. Keep the cable loop left of the
  antenna and secure it away from key sockets and screw posts.
- Fix the upper board to sloped seats or angle brackets. Its four mounting
  holes do not align vertically with the lower holes; straight shared
  standoffs are not the intended mounting method.

The lower board's antenna aperture and two-layer keepout remain. Space to
the upper copper is included in the nominal study, but RF performance is
unmeasured. Keep metal, wiring and screws out of the antenna volume.

Measure the purchased core board, female socket height, plug engagement,
IDC housings, button leads and display assembly before locking these heights.
The PCB footprint excludes keycaps, controls, enclosure walls, feet and USB
plug clearance. Provide a removable top for unplugging the core board and
support hot-swap sockets with the switch plate. Existing STL/CAD models have
not been regenerated; the new enclosure must follow this stacked assembly.

## Hand Assembly

1. Cut and deburr the bare panel, then clean both boards before fitting parts.
2. Solder the 0805 parts and 18 SOD-123 diodes first. Check every diode stripe.
3. Solder Kailh sockets on the back, keeping the iron and solder off plastic.
   Support each socket flat on the PCB; inspect both contacts for wetting.
4. Fit the two 1x22 female sockets using the unpowered core board as an
   alignment jig. Tack one end of each row, check seating, then finish.
5. Fit J8 on the lower top and J9 on the upper bottom, then the
   display/expansion headers and external buttons or wires.
6. Inspect bridges and polarity and measure supply-to-ground resistance
   before plugging in the core board or applying power. Verify all 20 cable
   connections pin-for-pin and confirm supply polarity at upper-board J2.

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

Schematic, panel PCB, project, BOM, placement manifest, interconnect and key-coordinate CSV are
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

Add `--view upper` or `--view lower` to the placement command, using new
output filenames `upper.kicad_pcb` / `lower.kicad_pcb`, to inspect the trimmed
boards in their assembled orientation. These are derived review files,
not separate sources to edit. Generate the nominal stack study with:

```sh
uv run --script scripts/hardware/preview_workbench_s3_stack.py \
  /tmp/workbench-s3-review/placement.json /tmp/workbench-s3-review
```

## Sources

- `refer/YD-ESP32-S3/README.md`, manufacturer P1/P2 pin tables.
- `refer/YD-ESP32-S3/5-public-YD-ESP32-S3-Hardware info/ESP32-S3-Metric.pdf`.
- `refer/YD-ESP32-S3/5-public-YD-ESP32-S3-Hardware info/YD-ESP32-S3-SCH-V1.4.pdf`.
- Original key geometry: `scripts/modeling/integrated_workstation.py`.
- Kailh socket footprint/model from foostan/kbd, copied from the RP2040
  draft with its MIT license in `licenses/`.
- Standard socket, diode and hand-solder footprints: KiCad 10.0.6 libraries.
- User-supplied module drawings: `references/display-dimensions.png` and
  `references/display-interface.png`, supplied on 2026-09-05.
