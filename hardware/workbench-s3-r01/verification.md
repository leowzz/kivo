# S3 Draft Verification - 2026-09-05

KiCad CLI and pcbnew 10.0.6 on macOS. This is a review of the carrier's
electrical definitions and placement, not approval for fabrication.

| Check | Result |
| --- | --- |
| Schematic load, SVG and XML netlist export | Pass |
| ERC, including warnings | 0 violations |
| Exported schematic versus PCB pads | 137 connected pins and 46 nets match |
| Carrier schematic components | 47, plus 5 board-only mounting holes |
| YD P1/P2 pin mapping and socket orientation | Pass against manufacturer references |
| Module sockets | 2 x 22 PTH; 2.54 mm pitch; 25.40 mm row spacing; 1.0 mm drills |
| All 18 key/diode paths | Pass; column -> switch -> diode A -> K -> row |
| Key matrix | Rows GPIO4/5/6, columns GPIO7-12; 9 GPIOs |
| Display harness and optional pull-ups | Pass; J2 map and R1/R2 DNP checked |
| BOOT/RESET | Pass; J5 GPIO0/GND, J6 EN/GND |
| Expansion | Pass; GPIO1/2/38/39/40/41/42/47, 3V3 and GND |
| Hand-solder packages | SOD-123 diodes; 0805 extended pads; through-hole connectors |
| Board size and stack | 126 x 135 mm; 1.6 mm; 2 copper layers |
| Key positions | 18 positions match CSV; 19.05 mm spacing retained |
| Antenna | Copper/pad/track/via/footprint keepout on both layers; opening inspected |
| Top/bottom renders and schematic | Visually inspected |
| DRC placement/clearance/silkscreen violations | 0 |
| Unconnected items | **91; routing has not started** |
| Copper tracks and pours | **0 tracks, no copper pours; one rule-area zone** |
| Installed core-board/display 3D fit and RF performance | **Not verified** |
| Enclosure and panel | **New design required; old models not modified** |
| ESP32-S3 display and diode-aware multi-key firmware | **Not implemented** |
| Manufacturing outputs | **Not generated** |

The core board's 57.15 mm PCB length, 63.39 mm total length, 27.94 mm width,
25.40 mm header-row spacing and 53.34 mm pin-1-to-pin-22 span were checked
visually in the manufacturer metric PDF. Pin numbering, 3.3 V supply,
BOOT/EN and the 5V input diode/jumper were checked against the V1.4 schematic
and manufacturer README. Actual purchased-board dimensions, male pin seating
and female socket height still require a fit check.

The carrier deliberately avoids native USB pins, UART bridge pins, PSRAM
pins and unused strap pins. Its P1 pin 21 is unconnected, so the reference
board's input-only 5V header is not used as a USB-powered output.

Current firmware evidence:

- `src/main.cpp` drives one matrix row low and reads pulled-up columns.
- `src-tauri/src/product.rs::matrix_partitions` puts the lowest pin of the
  connected matrix on the row side. GPIO4-6 are the row partition here.
- `lib/gpio_trigger/src/GpioTriggerController.cpp::createsContactCycle`
  still filters contact cycles; the diode board needs explicit firmware
  support before full rollover can be claimed.
- `src/platform/esp32s3.cpp::configureDisplay` rejects configured displays.
  `lib/gpio_trigger/src/BoardProfile.h` also disables OLED for ESP32-S3 and
  excludes GPIO39-42. No firmware or released product changes were made.

Generated evidence is in ignored `output/hardware/workbench-s3-r01/`:
`erc.json`, `drc.json`, `netlist.xml`, `board-top.png`, `board-bottom.png`,
and `schematic/workbench-s3-r01.svg`.

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
