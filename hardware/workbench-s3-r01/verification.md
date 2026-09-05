# S3 Stacked Draft Verification - 2026-09-05

KiCad CLI and pcbnew 10.0.6 on macOS. This is a review of the carrier's
electrical definitions and placement, not approval for fabrication.

| Check | Result |
| --- | --- |
| Schematic load, SVG and XML netlist export | Pass |
| ERC, including warnings | 0 violations |
| Exported schematic versus panel PCB pads | 185 connected pins and 65 nets match |
| Carrier schematic components | 52, plus 12 board-only mounting holes and 6 mouse-bite footprints |
| Separate upper-board connectivity | 45 parts, 119 connected pins, 37 nets |
| Separate lower-board connectivity | 7 parts, 66 connected pins, 28 nets |
| Board-to-board cable | J8/J9, 4 pins, 1:1 GND/3V3/SDA/SCL; mapping matches CSV and schematic |
| Cable connector geometry | JST XH 1x4, 2.50 mm pitch, 0.95 mm PTH drills; underside mirroring and panel rotation checked |
| Circuit separation | No net shared by the two board sections; upper nets prefixed `UP_` |
| YD P1/P2 pin mapping and socket orientation | Pass against manufacturer references |
| Module sockets | 2 x 22 PTH; 2.54 mm pitch; 25.40 mm row spacing; 1.0 mm drills |
| All 18 key/diode paths | Pass; column -> switch -> diode A -> K -> row |
| Key matrix | U1 GPA5/6/7 rows, GPB0-5 columns; only GPIO13/14 on ESP32 for all application inputs and display |
| MCP23017 SOIC pin mapping | Pass; VDD9/VSS10, SCL12/SDA13, A0/A1/A2 grounded at 15/16/17, RESET18 pulled up by R4 |
| MCP23017 input restriction | GPA7 used as row output; GPB7 unused; neither is assigned as input |
| U1 supply and I2C pull-ups | C3 100n near VDD, C1 10u/C2 100n, R4 10k RESET, R1/R2 fitted 2.2k; actual bus rise time pending |
| Display harness | Reference order retained: VCC, GND, Back, B, A, Press, SCL, SDA, Confirm; controls now GPA4/3/2/1/0 |
| Display connector position | Horizontal, front view; rightmost pin 1 at (39.70,4.93), leftmost pin 9 at (19.38,4.93) mm on upper PCB |
| Display mechanical reference | 64.90 x 35.03 mm envelope at (8,3); 4 additional M3 mounts match the supplied asymmetric centers |
| Display header pitch | 2.54 mm assumed standard; not explicitly dimensioned in source image; physical measurement pending |
| BOOT/RESET | Pass; J5 GPIO0/GND, J6 EN/GND |
| Expansion | Pass; 22 spare GPIOs on J4 1x24, plus 3V3/GND; 2.54 mm pitch and 1.0 mm drills |
| Hand-solder packages | SOIC-28 wide, 1.27 mm lead pitch, no thermal pad; SOD-123; 0805 extended pads; through-hole connectors |
| Panel size and stack | 126 x 187 mm; 1.6 mm; 2 copper layers |
| Finished boards | Upper 126 x 98 mm; lower 126 x 86 mm |
| Panel outline | One connected outer polygon, two internal separation slots and one antenna aperture |
| Breakaway tabs | Three 5 mm nominal necks; 36 NPTH holes, 0.5 mm drill / 0.8 mm pitch |
| Trim-band protection | Tracks/vias/pours forbidden on both layers; electrical pads verified outside Y 96-103 |
| Lower-board transform | 180 degrees in the panel; socket and XH pad numbering checked after rotation |
| Key positions | 18 positions match CSV; 19.05 mm spacing retained |
| Antenna | Copper/pad/track/via/footprint keepout on both layers; opening inspected |
| Panel top/bottom, individual boards, stack side view and schematic | Visually inspected |
| DRC placement/clearance/silkscreen violations | 0 |
| Unconnected items | **120 total: 82 upper, 38 lower; routing has not started** |
| Copper tracks and pours | **0 tracks, no copper pours; two panel rule-area zones** |
| Nominal stacking calculation | 30 degrees; front underside Z 25 mm, rear Z 74 mm; upper PCB projected depth 85.67 mm |
| Nominal clearances | Core 14.69 mm, antenna 16.33 mm normal to upper plane/parts; upper XH to core 38.18 mm vertically |
| Cable endpoint distance | 64.92 mm chord; 150 mm four-wire harness nominal, service loop and bend radius not fitted |
| Installed core-board/display 3D fit and RF performance | **Not verified** |
| Enclosure and switch plate | **New stacked design required; old models not modified** |
| Manufacturer panelization review | **Pending: slot/drill/tab process and two-design pricing** |
| MCP23017 scanner, encoder scheduling, S3 display and diode-aware multi-key firmware | **Not implemented; 1 kHz polling / chunked OLED refresh is a target only** |
| Manufacturing outputs | **Not generated** |

The core board's 57.15 mm PCB length, 63.39 mm total length, 27.94 mm width,
25.40 mm header-row spacing and 53.34 mm pin-1-to-pin-22 span were checked
visually in the manufacturer metric PDF. Pin numbering, 3.3 V supply,
BOOT/EN and the 5V input diode/jumper were checked against the V1.4 schematic
and manufacturer README. Actual purchased-board dimensions, male pin seating
and female socket height still require a fit check.

U1's physical SOIC pinout and GPA7/GPB7 output-only limitation were checked
against Microchip DS20001952D page 1, and the pin/bias table on page 11.
The upper control inputs occupy GPA0-GPA4, leaving GPA5-GPA7 for output rows.
INTA/INTB are intentionally unconnected to avoid a fifth harness conductor.
The U1 footprint has 28 exposed side pads and no center pad. Address 0x20
and OLED 0x3C/0x3D must be confirmed during powered bring-up. Bus rise time,
actual module pull-ups, supply current and encoder performance are unmeasured.

The stack study assumes maximum core/socket top Z = 20 mm, antenna top
Z = 14.5 mm, underside key/diode height 3.5 mm and mated XH height 12 mm.
It checks nominal geometric envelopes, not manufacturing tolerances, the
actual display/control assembly, USB plugs, cable service loop, RF behavior,
keycaps or printed enclosure. A different socket height or XH housing
requires a fresh fit calculation. The separated boards' four mounting holes
are independently supported; tilted upper holes do not align with vertical
lower-board standoffs.

The carrier deliberately avoids native USB pins, UART bridge pins, PSRAM
pins and unused strap pins. Its P1 pin 21 is unconnected, so the reference
board's input-only 5V header is not used as a USB-powered output.

Current firmware evidence:

- `src/main.cpp` currently scans matrix contacts through MCU GPIOs.
  There is no MCP23017 input backend. The new port names cannot be loaded
  through the existing direct-GPIO product schema; the YAML is a separate
  hardware contract, not a runnable product profile.
- `lib/gpio_trigger/src/GpioTriggerController.cpp::createsContactCycle`
  still filters contact cycles; the diode board needs explicit firmware
  support before full rollover can be claimed.
- `src/platform/esp32s3.cpp::configureDisplay` rejects configured displays.
  `lib/gpio_trigger/src/BoardProfile.h` also disables OLED for ESP32-S3 and
  excludes GPIO39-42. No firmware or released product changes were made.
- Shared-bus scheduling must sample both encoder channels together, scan
  all rows, and split OLED writes into short transactions. The 400 kHz bus,
  1 kHz scan and 8-byte OLED chunks in the draft require timing and functional
  validation; passing these file checks does not prove polling reliability.

Generated evidence is in ignored `output/hardware/workbench-s3-r01/stacked/`:
`erc.json`, `drc.json`, `netlist.xml`, `panel-top.png`, `panel-bottom.png`,
`upper.kicad_pcb`, `lower.kicad_pcb`, individual-board renders and DRC reports,
`stack-side.png`, `stack-side.svg`, `stack-clearances.json`, and
`schematic/workbench-s3-r01.svg`. The supplied display reference images are
retained under `hardware/workbench-s3-r01/references/`. The individual PCBs are derived trimmed
previews; the panel PCB in this source directory is the editable master.

Re-run from the repository root:

```sh
/Applications/KiCad/KiCad.app/Contents/MacOS/kicad-cli sch erc \
  --format json --exit-code-violations -o /tmp/workbench-s3-erc.json \
  hardware/workbench-s3-r01/workbench-s3-r01.kicad_sch

/Applications/KiCad/KiCad.app/Contents/MacOS/kicad-cli sch export netlist \
  --format kicadxml -o /tmp/workbench-s3-netlist.xml \
  hardware/workbench-s3-r01/workbench-s3-r01.kicad_sch

/Applications/KiCad/KiCad.app/Contents/Frameworks/Python.framework/Versions/3.9/bin/python3.9 \
  scripts/hardware/verify_workbench_s3.py hardware/workbench-s3-r01 /tmp/workbench-s3-netlist.xml

/Applications/KiCad/KiCad.app/Contents/MacOS/kicad-cli pcb drc \
  --format json --exit-code-violations -o /tmp/workbench-s3-drc.json \
  hardware/workbench-s3-r01/workbench-s3-r01.kicad_pcb
```

The last command is expected to fail until routing is complete. The focused
verification script deliberately asserts this draft has no tracks; update
that assertion when routing begins. Passing ERC does not validate the
preassembled module's internal circuitry or real hardware behavior.

Use `--individual-boards output/hardware/workbench-s3-r01/stacked` on the
verification command to include the two derived boards. See `README.md`
for reproducing them and the stack envelope calculation. No DRC violations
were excluded to make this panel pass; unconnected items are still reported.
