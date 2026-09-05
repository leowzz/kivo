# Verification - 2026-09-05

Tools: KiCad CLI and pcbnew 10.0.6, Freerouting 1.9.0. This is an independent
keyboard-only hardware revision with an external ESP32-S3 connection.

| Check | Result |
| --- | --- |
| Schematic load, SVG and XML netlist export | Pass; one A3 root sheet with numbered page instance |
| ERC, including warnings | 0 violations |
| Final PCB DRC, including warnings | 0 violations, 0 unconnected items |
| Schematic / manifest / PCB pin parity | 103 connected pins and 32 nets match |
| Explicit no-connect parity | All 11 unused/NC U1 pins match; no connected net is discarded |
| Electrical components | 43: 18 sockets, 18 diodes, U1, J1, C1-C2, R1-R3 |
| Mechanical components | Four M3 clearance holes; no module/display mounts or breakaway tabs |
| Board | 124 x 76 mm, two copper layers, 1.6 mm thick |
| Key positions | Six columns and three rows, 19.05 mm pitch, 18 total |
| Key circuit | Every column -> switch -> diode anode -> diode cathode -> row path checked |
| MCP23017 | SOIC-28W, all 28 pads SMD; address 0x20; A0-A2 grounded, RESET pulled high |
| Output-only pins | GPA7 is a row output; GPB7 unused |
| J1 footprint | 1x4 vertical female socket, 2.54 mm pitch, four 1.0 mm PTH drills |
| J1 physical pad coordinates | Pin 1 at (90,6); pins 2-4 at +2.54 mm X increments |
| J1 silkscreen | Each front/back signal label aligned to the corresponding physical pad; underside text mirrored correctly |
| Connector nets | 1 GND, 2 3V3, 3 SDA, 4 SCL; matches interconnect.csv and netlist |
| Copper | 313 track segments, 20 through vias, two filled ground pours |
| Routing dimensions | 0.25 mm signals, 0.5 mm power, 0.4 mm local decoupling; 0.2 mm clearance |
| Board-edge clearance | Minimum 0.5 mm copper setback |
| Via geometry | 0.6 mm diameter / 0.3 mm drill; tenting specified |
| Screw clearance | Four 3 mm radius track/via/pour keepouts on both copper layers |
| C2 local decoupling | Short, locked front-layer routes to adjacent U1 pins 9/10, no vias |
| Drill files | 24 plated holes (20 vias + 4 connector pads), 94 non-plated holes |
| Manufacturing export | Nine Gerber layers, job file, separate PTH/NPTH drills and ZIP |
| Visual inspection | Schematic and final front/back renders inspected |
| Physical assembly | Pending purchased parts, switch plate, female header and enclosure fit |
| Powered validation | Pending continuity, power, I2C, debounce and all-key checks on hardware |
| Firmware | MCP23017 scanner not implemented by this hardware change; direct-GPIO firmware is incompatible |

The two new scripts validate the electrical manifest against the exported
schematic and actual PCB pads, including the explicit no-connect pin set.
They also check connector pitch, hole size, mounting side, key paths and
the correspondence between front/back connector labels and pad positions.
Ground pours are filled before final DRC and manufacturing export.

The DRC configuration preserves the original project's severity categories,
with no individual exclusions. Previously ignored categories remain
footprint_filters_mismatch, footprint_type_mismatch, missing_courtyard,
track_not_centered_on_via and tuning_profile_track_geometries. Copper, shorts,
unconnected items, holes, courtyards, silkscreen and board-edge checks run.

The imported socket footprint and local STEP model retain the original
project's library and license attribution. Rendered parts are nominal library
models. Neither the model nor DRC establishes purchased-part tolerances,
insertion force support, switch plate spacing, connector engagement or keycap
clearance. The selected female connector must accept standard 0.64 mm square
pins at 2.54 mm pitch. S3 wiring is by individual signal labels, not four
consecutive pins, and uses 3.3 V only.
