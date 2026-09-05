# Workbench S3 r01 - Stacked Breakaway Boards

Independent **routed prototype**, using the YD ESP32-S3 core board from
`refer/YD-ESP32-S3/`. Open `workbench-s3-r01.kicad_pro` in KiCad 10.
The RP2040 design remains in `../workbench-r03/`. Physical module fit and
the fabricator's panel process still require confirmation; this is not a
released product definition. Firmware support remains pending.

## Architecture

- Manufacture a single mechanically connected 126 x 187 mm panel, then cut
  the perforated tabs into an upper key board and a lower controller board.
- Assemble the 126 x 86 mm lower board horizontally and the 126 x 98 mm upper
  board rear-high at a nominal 30 degrees. The PCB projection fits in 86 mm
  depth, compared with 135 mm for the previous single-board draft.
- J8 on the lower board and J9 on the upper underside connect by one **four-wire
  harness: GND, 3V3, SDA, SCL**. No electrical connection crosses the tabs.
- Preassembled YD ESP32-S3 plugs into two 1x22 female sockets, J1 and J7.
- Reuse the core board's native USB Type-C for HID/data and power.
- 18 MX hot-swap keys use a 3x6 matrix with one 1N4148W diode per key.
- An upper-board MCP23017 scans the matrix and five control inputs; it shares
  the display's I2C bus. The application uses just **two ESP32 GPIOs**, 13/14.
- SH1106/EC11/confirm/back module connects by the J2 harness.
- J4 exposes 22 general-purpose spare GPIOs, GND and 3.3 V.
- J5/J6 provide external BOOT/RESET button connections.
- Carrier parts are through-hole connectors, SOD-123 diodes, 0805 passives
  with extended pads, and one SOIC-28 wide chip with 1.27 mm lead pitch.
- No bare MCU, flash, crystal, regulator or fine-pitch USB connector needs
  soldering on this carrier.

## Schematic Pages

Open `workbench-s3-r01.kicad_sch` as the root schematic. Page 1 (A4) contains
the lower board's core sockets, expansion header, BOOT/RESET and J8 cable
connector. Its `Upper` sheet opens page 2 (A3), containing the upper board's
power entry, MCP23017 with local decoupling/reset/I2C pull-ups, display
connector and directly wired 3x6 key/diode matrix. Both schematic files are
required when copying the project.

The upper sheet uses absolute global labels such as `/UP_GPA5` to preserve
the existing routed PCB net names. They do not connect to the lower board;
the external J8/J9 harness still joins the two independent circuits. The
layout retains all component values, footprints, reference designators and
symbol UUIDs. The PCB's upper-component schematic paths are updated for the
new hierarchy; copper, footprints and board geometry are unchanged.

`layout_workbench_s3_schematic.py` provides the shared page layout used by
the generator. `placement.json` records the two schematic paths so a fresh
placement keeps the correct symbol associations. A before/after exported
netlist can be checked using `verify_workbench_s3_schematic.py`.

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

| Function | MCP23017 Ports |
| --- | --- |
| Rows R0-R2, selected LOW, inactive HIGH | GPA5, GPA6, GPA7 |
| Columns C0-C5, read with internal pull-ups | GPB0, GPB1, GPB2, GPB3, GPB4, GPB5 |

K1-K6 are the rear key row, K7-K12 the middle, and K13-K18 the front.
For every key: column -> switch -> diode anode A -> diode cathode K -> row.
**The stripe on each diode goes to the row net.** D1 belongs to K1, and so on.
Use 1N4148W in SOD-123, not the smaller SOD-323 version.

U1 is **MCP23017-E/SO**, at I2C address **0x20** (A0/A1/A2 tied to ground).
Its SOIC-28 wide package has accessible 1.27 mm pitch leads and no underside
thermal pad; assemble with a soldering iron, flux and solder wick. Do not
substitute MCP23S17 (SPI), SSOP or QFN for this footprint. R4 holds RESET
high, C3 is U1's local 100 nF decoupler, and C1/C2 decouple the upper supply.

The current Microchip datasheet specifies **GPA7 and GPB7 as output-only**.
GPA7 is a row output here; GPB7 is unused. GPB6 is also unused. GPA0-GPA4
read the display module's five control signals. INTA/INTB are unconnected;
firmware polls the ports so the harness needs no interrupt conductor.

This changes total application use from 16 MCU GPIOs to 2, and the inter-board
cable from 20 conductors to 4. Diodes prevent electrical ghost paths; firmware
must still implement matrix debounce and rollover behavior. Configure unused
ports to defined states and preload all row output latches HIGH before
enabling row outputs, then select one LOW at a time.

## Other Interfaces

All pin orders below use each separated board's front/component-side view,
rear edge at the top. The lower board is rotated 180 degrees in the panel.

| J2 Pin | Signal | Destination |
| --- | --- | --- |
| 1, rightmost square pad | VCC / 3.3 V | - |
| 2 | GND | - |
| 3 | BAK / KEY0 / Back | U1 GPA4 |
| 4 | TRB / TRIM_B / Encoder B | U1 GPA3 |
| 5 | TRA / TRIM_A / Encoder A | U1 GPA2 |
| 6 | PSH / PUSH / Encoder press | U1 GPA1 |
| 7 | SCL / IIC_SCL | ESP32 GPIO14 / U1 SCL |
| 8 | SDA / IIC_SDA | ESP32 GPIO13 / U1 SDA |
| 9, leftmost pad | CON / KEY1 / Confirm | U1 GPA0 |

J2 matches the supplied module interface diagram and front photograph in
`references/display-interface.png`. Looking at the readable display with
the encoder on the right, the top connector reads **CON, SDA, SCL, PSH,
TRA, TRB, BAK, GND, VCC from left to right**. This is pin 9 through pin 1.
VCC is **3.3 V only**. Use a pin-for-pin 9-wire harness and identify the
square pad rather than inferring pin numbers from a rear or mating-face view.
The module-side order is unchanged by this I2C revision; only the five
control signals' carrier-side destinations move from ESP32 GPIOs to U1.

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

R1/R2 are fitted **2.2 kohm I2C pull-ups to 3.3 V**. Include the display's
parallel pull-ups when calculating the effective resistance. For example,
2.2k in parallel with 4.7k is about 1.5k. Verify low-level sink current and
rise time with both harnesses attached; target at most 300 ns rise time at
400 kHz. Do not connect a module whose I2C pull-ups go to 5 V.

J4 sits at the lower board's rear-left edge. Its 1x24, 2.54 mm pitch plated holes have
1.0 mm drills. From the left square pad to the right:

| Pins | Signals In Pin Order |
| --- | --- |
| 1-6 | GND, 3V3, GPIO1, GPIO2, GPIO4, GPIO5 |
| 7-12 | GPIO6, GPIO7, GPIO8, GPIO9, GPIO10, GPIO11 |
| 13-18 | GPIO12, GPIO15, GPIO16, GPIO17, GPIO18, GPIO21 |
| 19-24 | GPIO38, GPIO39, GPIO40, GPIO41, GPIO42, GPIO47 |

Pin 1 is at `(10,3)` mm and pin 24 at `(68.42,3)` mm. The two button
headers move right to leave this longer expansion row clear: BOOT pin 1
at `(73,4)` and RESET pin 1 at `(80,4)` mm.

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

Use two **JST XH B4B-XH-A, 1x4, 2.50 mm pitch, vertical through-hole
headers** and matching XHP-4 housings/crimp contacts. Buy a precrimped,
pin-for-pin four-conductor harness to avoid soldering cable contacts.
Start with 150 mm including service slack. This is **2.50 mm XH**, not a
2.54 mm pin-header or IDC footprint; the PCB drills are 0.95 mm. Match the
actual header and housing dimensions before buying compatible parts.

J8 mounts on the lower board's top; J9 mounts on the upper board's bottom.
They face into the space between boards. The connection is **pin 1 to pin 1,
through pin 4 to pin 4**. Use the square PCB pad and the actual connector
pin numbering, not wire colors or a mirrored mating-face view. Prewired
cables can reverse the pin order: check continuity before powering the assembly.
J8 pin 1 is at lower `(47,25)` and J9 pin 1 at upper `(83,8)` mm.
Viewed through each board from the front, pin numbers increase towards +X;
the underside mating view is mirrored. The keyed housings face into the stack.

| Pins | Signals In Pin Order |
| --- | --- |
| 1-4 | GND, 3V3, SDA / GPIO13, SCL / GPIO14 |

`interconnect.csv` is the complete pin-by-pin mapping. Upper-board schematic
nets use `UP_` prefixes, including `UP_GND` and `UP_3V3`; lower-board nets use
the original names. The external cable joins these separate circuit domains.
Never merge the names or route across the breakaway tabs to remove airwires.
Start bring-up at 100 kHz, then validate 400 kHz with the actual harnesses.
The OLED and MCP23017 share this bus; confirm the OLED's 0x3C/0x3D address
and the expander's 0x20 address. Four conductors meet the minimum for this
shared I2C bus plus supply/ground. Further reduction would need a different
protocol or additional active circuitry and would complicate this assembly.

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

All 12 mounting holes have a 3.0 mm radius keepout on both copper layers,
covering the specified maximum 5.6 mm screw heads. Use matching small-head
M3 screws; larger heads or washers need a new clearance check.

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
  Z = 14.5 mm, upper underside parts of 3.5 mm, and mated XH envelopes
  of 12 mm. Under these assumptions, core-to-upper-parts clearance is
  14.69 mm normal to the upper plane; antenna clearance is 16.33 mm.
- The upper XH envelope clears the core envelope vertically by 38.18 mm.
  J8 is beside the core, not underneath it. Keep the cable loop left of the
  antenna and secure it away from key sockets and screw posts.
- Fix the upper board to sloped seats or angle brackets. Its four mounting
  holes do not align vertically with the lower holes; straight shared
  standoffs are not the intended mounting method.

The lower board's antenna aperture and two-layer keepout remain. Space to
the upper copper is included in the nominal study, but RF performance is
unmeasured. Keep metal, wiring and screws out of the antenna volume.

Measure the purchased core board, female socket height, plug engagement,
XH housings, button leads and display assembly before locking these heights.
The PCB footprint excludes keycaps, controls, enclosure walls, feet and USB
plug clearance. Provide a removable top for unplugging the core board and
support hot-swap sockets with the switch plate. Existing STL/CAD models have
not been regenerated; the new enclosure must follow this stacked assembly.

## Hand Assembly

1. Cut and deburr the bare panel, then clean both boards before fitting parts.
2. Solder U1 on the upper front first: align pin 1 with its marker, tack
   opposite corners, then solder the visible SOIC leads and inspect for
   bridges. Fit 0805 parts and 18 SOD-123 diodes; check every diode stripe.
3. Solder Kailh sockets on the back, keeping the iron and solder off plastic.
   Support each socket flat on the PCB; inspect both contacts for wetting.
4. Fit the two 1x22 female sockets using the unpowered core board as an
   alignment jig. Tack one end of each row, check seating, then finish.
5. Fit J8 on the lower top and J9 on the upper bottom, then the
   display/expansion headers and external buttons or wires.
6. Inspect bridges and polarity and measure supply-to-ground resistance
   before plugging in the core board or applying power. Verify all four cable
   connections pin-for-pin and confirm supply polarity at upper-board J2.

Buy one preassembled YD ESP32-S3 core board and two matching 1x22 male strips
if the core board is supplied without pins. `bom-draft.csv` lists the carrier
parts; the purchased core board, switch bodies, keycaps and display module
are separate assemblies. Ordinary soldering-iron assembly is intended.

## Routing

- Two copper layers, 0.25 mm signal tracks, 0.5 mm power branches and
  0.4 mm local power connections/socket escape. Minimum clearance is 0.2 mm.
- Through vias are 0.6 mm diameter with 0.3 mm drills, tented on both sides.
  Copper stays at least 0.5 mm from finished routed edges.
- C3 moves to `(90.5,23.905)` mm and rotates 180 degrees so its supply pad
  faces U1 pin 9. Its supply/ground connections form a short, via-free loop
  on the front layer; the capacitor value and electrical pin mapping are unchanged.
- Each finished board has its own front/back ground pours and stitching vias.
  Pours use 0.25 mm clearance and 0.3 mm thermal spokes for soldering access;
  floating islands are removed. No copper or electrical net crosses the tabs.
- The lower antenna aperture/keepout and upper display geometry are retained.
  BOOT, RESET, all 22 expansion GPIOs and the four-wire harness are routed.

The PCB is the editable routed master. The placement generator deliberately
still creates an unrouted board; never run it over this master. Recheck DRC
after every subsequent footprint, outline or copper edit.

## Manufacturing Files

The routed panel and both extracted boards pass KiCad 10.0.6 DRC with
**0 violations and 0 unconnected items**. The panel contains 664 track
segments, 102 through vias and four filled ground pours.

Generated files are under `output/hardware/workbench-s3-r01/stacked/`
from the repository root. `workbench-s3-r01-gerbers.zip` contains only the
panel's Gerber layers, job file and separate plated/non-plated drill files.
The individual-board previews are not extra designs in that ZIP.

Specify two-layer, 1.6 mm FR-4, 1 oz copper and tented vias. `Edge.Cuts`
contains the outline, routed separation slots and antenna aperture;
the NPTH file contains the mouse bites and other mechanical holes.
There are 191 plated holes (including 102 vias) and 138 non-plated holes.
No V-scoring is required. Paste layers are supplied but a stencil is
optional for the intended soldering-iron assembly.

Before ordering, measure the assumed 2.54 mm display header pitch and
confirm the actual module/socket fit. Ask the fabricator to review the
0.5 mm perforation drills, 0.8 mm hole pitch, tab strength and two-design
panel policy. Firmware and the new stacked enclosure remain separate work.

## Firmware Status

`firmware-profile-draft.yaml` is a hardware wiring contract, **not a loadable
Kivo product/profile schema**. It is deliberately outside `products/`.
The current direct-GPIO matrix schema cannot represent MCP23017 port names.
This hardware revision requires new firmware; the r02 product cannot run it.

- `src/platform/esp32s3.cpp` currently has no display implementation, and
  `kYdEsp32S3.supportsOled` is false. SH1106 support must be implemented.
- Implement an MCP23017 backend with GPA5-GPA7 output rows, GPB0-GPB5
  pulled-up column inputs and GPA0-GPA4 pulled-up control inputs. Read both
  encoder channels together from GPIOA; avoid output read-modify-write on
  that mixed port and maintain an OLATA shadow. Handle I2C faults and
  expander power-on/reset without issuing false key events.
- Target about 1 kHz full matrix scans and control polling at 400 kHz I2C.
  Fragment OLED refreshes (initial target: at most 8 data bytes per transaction)
  and give input polling priority. A full blocking OLED frame would prevent
  sampling for tens of milliseconds and can lose encoder transitions.
  Measure the worst sampling gap and encoder response during refreshes and
  USB/Wi-Fi load; polling performance is not yet validated. Reading INTCAP
  alone is not an encoder event queue and cannot recover every missed edge.
- The contact-cycle filter needs explicit diode-matrix support before
  claiming full rollover. Add debounce and simultaneous-key tests to the
  new backend; the current direct-GPIO scanner is not used unchanged.
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

For routed individual-board previews, use the routed master instead of the
placement generator:

```sh
/Applications/KiCad/KiCad.app/Contents/Frameworks/Python.framework/Versions/3.9/bin/python3.9 \
  scripts/hardware/route_workbench_s3.py upper \
  hardware/workbench-s3-r01/workbench-s3-r01.kicad_pcb /tmp/upper.kicad_pcb

/Applications/KiCad/KiCad.app/Contents/Frameworks/Python.framework/Versions/3.9/bin/python3.9 \
  scripts/hardware/route_workbench_s3.py lower \
  hardware/workbench-s3-r01/workbench-s3-r01.kicad_pcb /tmp/lower.kicad_pcb
```

`route_workbench_s3.py` also provides `prepare`, `import --session`, and
`finish` stages for reproducing local Specctra routing from a fresh placement.
Each stage preserves the matching `.kicad_pro` rules beside its output PCB
and refuses to overwrite an existing board or project.
`prepare` configures signal/power net classes, critical supply tracks and
screw keepouts. Its DSN includes temporary board-edge guards because KiCad
does not export copper-to-edge clearance. `finish` adds and refills the
independent ground pours and stitching vias. Always run KiCad DRC on the
result: the autorouter's completion report is not the final check.

This revision used the local Freerouting 1.9.0 release with
`-mp 50 -mt 1 -oit 0.2 -da`. The JAR SHA256 is
`9084a4888937a7f31f857ecc12aa7a37407f51160e4d2892dff9c9bb47ae3102`.
The exported DSN and resulting SES are retained in the ignored evidence
directory's `routing/` subdirectory. Java requires an available AWT display
for this version, even when routing is started from the command line.

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
- Microchip [MCP23017 datasheet DS20001952D](https://ww1.microchip.com/downloads/aemDocuments/documents/APID/ProductDocuments/DataSheets/MCP23017-Data-Sheet-DS20001952.pdf),
  pages 1/11 for SOIC pinout, output-only GPA7/GPB7 and address/reset bias;
  pages 6-7 for 400 kHz timing. U1 uses the SOIC pinout, not QFN numbering.
