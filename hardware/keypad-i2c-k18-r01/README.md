# Standalone 18-Key I2C PCB r01

Open `keypad-i2c-k18-r01.kicad_pro` in KiCad 10. This independent design is a
**routed 124 x 76 mm keyboard PCB**, with six columns and three rows of MX
hot-swap switches at 19.05 mm pitch. It connects to an external ESP32-S3 using
four individual Dupont wires. The older Workbench designs are separate projects.

The board has 43 electrical components: 18 sockets, 18 diodes, one MCP23017,
one four-position female connector, two capacitors and three resistors.
There is no controller carrier, display connector, USB port, regulator,
expansion connector, BOOT/RESET button, or board-to-board panel connection.
The four mounting holes are mechanical only.

## J1 Wiring

J1 is a **1x4 vertical female socket, 2.54 mm pitch**, for standard 0.64 mm
square male pins. Its four plated PCB holes have 1.0 mm drills. Purchase the
ordinary solder-tail female strip, not a 2.50 mm JST XH connector.

Viewed from the component/key side, with the chip and connector at the top,
pin 1 is the **leftmost square pad**. The signals run left to right:

| J1 pin | Signal | External YD ESP32-S3 connection |
| --- | --- | --- |
| 1 | GND | GND, manufacturer P1 pin 22 |
| 2 | 3V3 | 3V3, manufacturer P1 pin 1 or 2 |
| 3 | SDA | GPIO13, manufacturer P1 pin 19 |
| 4 | SCL | GPIO14, manufacturer P1 pin 20 |

Use **four separate male-to-female Dupont wires**: male ends into J1, female
ends onto the ESP32-S3's installed male header. The pitch matches the S3
header, but these four signals are not four consecutive S3 pins. Do not
plug the whole block across an arbitrary run of four GPIOs. Follow the
actual core board's printed signal labels; P1 numbering above applies to
the repository's YD ESP32-S3 reference board.

From the PCB underside, the square pad is at the right and the readable
silkscreen runs **SCL, SDA, 3V3, GND** from left to right. Each signal label
is aligned to its own pad. `interconnect.csv` records the same connections.

Power this PCB from the S3's **3.3 V**, never 5 V. No voltage regulator or
level shifting is present. Keep the four wires short, initially about
100-150 mm; start I2C at 100 kHz. Validate rise time before increasing to
400 kHz. R1/R2 provide 2.2 kohm pull-ups to 3.3 V. Account for any additional
pull-ups elsewhere on a shared bus; all pull-ups must go to 3.3 V.

## Keys And I2C Chip

U1 is **MCP23017-E/SO**, SOIC-28 wide with 1.27 mm exposed leads and no
underside thermal pad. Its fixed 7-bit I2C address is **0x20**, with A0-A2
tied to GND. Use this I2C part and SOIC package; MCP23S17 is SPI and uses a
different interface. R3 holds RESET high. C2 is the 100 nF local decoupler,
with short, via-free connections to U1 pins 9 and 10; C1 is 10 uF at J1.

| Function | Ports |
| --- | --- |
| Rows, rear to front | GPA5, GPA6, GPA7 |
| Columns, left to right | GPB0, GPB1, GPB2, GPB3, GPB4, GPB5 |
| Unused GPIO | GPA0-GPA4, GPB6, GPB7 |
| Unconnected interrupt outputs | INTA, INTB |

K1-K6 are the rear row, K7-K12 the middle row and K13-K18 the front row,
all numbered from the key side. D1 belongs to K1, and so on. Each path is
column -> switch -> diode anode -> diode cathode -> row. The stripe on the
**1N4148W SOD-123** diode faces its cathode pad 1, connected to the row.
The SOD-123 footprint is not interchangeable with SOD-323.

This is an I2C peripheral, not a USB keyboard by itself. Existing direct-GPIO
Kivo firmware cannot scan it unchanged. The hardware work does not add an
MCP23017 firmware backend. A scanner must preload GPA5-GPA7 output latches
HIGH, enable the row outputs, select one LOW at a time, and read GPB0-GPB5
with internal pull-ups. Debounce and simultaneous-key handling remain
firmware responsibilities. Set unused ports to defined states, for example
outputs LOW. Current MCP23017 silicon specifies GPA7 and GPB7 as output-only;
GPA7 is correctly used as a row output and GPB7 is unused here.

## Mechanical And Hand Assembly

- PCB: 124 x 76 mm, two copper layers, 1.6 mm FR-4, nominal 1 oz copper.
- Key centers: X = 14.375 + column * 19.05 mm; Y = 26 + row * 19.05 mm,
  with zero-based column/row indices. `key-coordinates.csv` lists all 18.
- Four 3.4 mm NPTH M3 mounting holes: (4,4), (120,4), (4,72), (120,72) mm.
  Coordinates are from the rear-left PCB corner in the front view.
- Screw-head copper keepouts have 3 mm radius. Use heads no larger than
  5.6 mm and no large washers; check enclosure bosses and tool access.
- U1, J1, R1-R3 and C1-C2 are on the front. All 18 hot-swap sockets and
  diodes are on the back. Keep clearance below sockets and diode bodies.
- Switches need a suitable switch plate and socket support during insertion.
  The previous enclosure is not automatically compatible with this outline.

Solder U1 first with flux, then the 0805 resistors/capacitors, SOD-123 diodes,
hot-swap sockets and J1. All are intended for soldering-iron assembly, with
no hot-air station or bare MCU assembly. Inspect U1 bridges and diode
polarity, then check 3V3-to-GND resistance and all four cable connections
before applying power. Actual socket, keycap, header and enclosure fit still
need physical validation; DRC and rendered component models cannot establish
that fit.

## Outputs And Verification

`verification.md` records the checks. The schematic, PCB, project, local
footprints, hot-swap STEP model, BOM and coordinate/wiring tables are the
editable design. KiCad's normal libraries supply the remaining footprints
and 3D models. The checked-in PCB is the routed master.

Generated files are in `output/hardware/keypad-i2c-k18-r01/` from the repository
root: top/bottom PNG previews, schematic SVG/PNG, ERC/DRC reports, drill report,
and `keypad-i2c-k18-r01-gerbers.zip`. The ZIP contains the nine manufacturing
Gerber layers, job file and separate plated/non-plated drill files. There is
one rectangular board, with no breakaway tabs or V-scoring. Specify tented
vias. Confirm actual parts and plate/enclosure fit before ordering.

## Reproduction

Generate into a fresh directory to preserve manual KiCad edits. The schematic
generator reuses the original S3 design's symbols and the repository's
schematic formatting helpers; the finished KiCad directory is self-contained
apart from the installed standard KiCad libraries.

```sh
uv run --script scripts/hardware/generate_keypad_i2c.py --output /tmp/keypad-i2c-review
/Applications/KiCad/KiCad.app/Contents/Frameworks/Python.framework/Versions/3.9/bin/python3.9 scripts/hardware/pcb_keypad_i2c.py place /tmp/keypad-i2c-review/placement.json /tmp/keypad-i2c-review/placement.kicad_pcb
java -jar /path/to/freerouting-1.9.0.jar -de /tmp/keypad-i2c-review/placement.dsn -do /tmp/keypad-i2c-review/routed.ses -mp 10 -mt 1 -da
/Applications/KiCad/KiCad.app/Contents/Frameworks/Python.framework/Versions/3.9/bin/python3.9 scripts/hardware/pcb_keypad_i2c.py finish /tmp/keypad-i2c-review/placement.json /tmp/keypad-i2c-review/placement.kicad_pcb --session /tmp/keypad-i2c-review/routed.ses --output /tmp/keypad-i2c-review/keypad-i2c-k18-r01.kicad_pcb
/Applications/KiCad/KiCad.app/Contents/MacOS/kicad-cli sch export netlist --format kicadxml -o /tmp/keypad-i2c-review/netlist.xml /tmp/keypad-i2c-review/keypad-i2c-k18-r01.kicad_sch
/Applications/KiCad/KiCad.app/Contents/Frameworks/Python.framework/Versions/3.9/bin/python3.9 scripts/hardware/pcb_keypad_i2c.py verify /tmp/keypad-i2c-review/placement.json /tmp/keypad-i2c-review/keypad-i2c-k18-r01.kicad_pcb --netlist /tmp/keypad-i2c-review/netlist.xml
```

Run KiCad ERC and DRC with warnings enabled after regeneration or edits.
Freerouting 1.9 needs a graphical Java environment even when invoked with
file arguments. Routing scripts do not replace electrical or assembly review.
