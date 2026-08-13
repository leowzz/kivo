# Workbench One P0

tscircuit proof-of-concept for Kivo's all-in-one voice and macro workstation.

## Included

- USB-C device input and a representative USB2512B two-port USB 2.0 hub
- ESP32-S3-WROOM-1 controller with native USB
- representative CM108B USB audio section and four-pin handset connector
- 18 mechanical switches in a diode-isolated 3-row by 6-column matrix
- 1x9 SH1106/EC11 control-module interface
- three latching mode switches and one hook-switch input
- 180 mm by 105 mm board with four 3.2 mm mounting holes

Display connector order is fixed:

```text
CON / SDA / SCL / PSH / TRA / TRB / BAK / GND / 3V3
```

## Preview

Use Node 24.18.0, then run:

```bash
cd hardware/workbench-one
npm install
npm run dev -- src/main.tsx
```

The tscircuit development server prints the local browser URL. Select the PCB
view to inspect the interaction surface.

Generate static PCB artifacts with:

```bash
npm run build:preview
```

Run focused verification with:

```bash
npm test
npm run typecheck
npm run build
```

## Design Notes

- `CON`, `PSH`, `TRA`, `TRB`, and `BAK` are active-low inputs with 10 kOhm
  pull-ups.
- `SDA` and `SCL` have 4.7 kOhm pull-ups to 3.3 V.
- `MODE1..MODE3` and `HOOK` are active-low inputs with 10 kOhm pull-ups.
- The ESP32-S3 appears as Kivo's controller USB function; CM108B appears as a
  separate USB audio function behind the same hub.
- Audio samples never pass through the ESP32-S3.

## Not Fabrication Ready

This package deliberately uses representative footprints for the ESP32-S3
module, USB2512B, CM108B, SH1106/EC11 module, toggle switches, and handset
connector. Do not order this board from the P0 output.

Before fabrication, verify exact packages and pin maps against selected part
numbers, reproduce each datasheet reference circuit, route and impedance-check
all USB differential pairs, validate crystals and power sequencing, review the
ESP32-S3 antenna keepout, measure the handset microphone and receiver, design
the analog bias/gain path, run full DRC/ERC, and check the board against the
physical enclosure.

The matching design document is
`docs/superpowers/specs/2026-08-13-workbench-one-pcb-design.md`.
