# Workbench One P0 PCB Design

## Purpose

Workbench One P0 is a tscircuit proof-of-concept for a desktop voice and macro
console. It combines a telephone handset, an 18-key mechanical keypad, a
1.3-inch SH1106 display module with an EC11 encoder and two buttons, three
toggle switches, and one USB cable to the host.

This revision is intended to evaluate PCB composition, subsystem boundaries,
and the tscircuit workflow. It is not approved for fabrication. The ESP32-S3,
USB hub, CM108B, display-module, and switch footprints are representative until
they are checked against exact manufacturer drawings and physical samples.

## User Experience

- Lifting the handset changes a dedicated hook-switch input. Kivo maps that
  edge to a Typeless-like start shortcut; replacing the handset emits the stop
  edge.
- Eighteen mechanical keys form a 6-column by 3-row macro pad. Each switch has
  its own diode so arbitrary multi-key input does not ghost.
- The SH1106 module displays important Kivo status. The EC11 rotates through
  menus; `CON` confirms and `BAK` goes back. `PSH` is the encoder push input.
- Three latching toggle switches expose persistent hardware modes to firmware.
- A single USB-C connection enumerates the Kivo controller and CM108B audio
  codec through an internal two-port USB 2.0 hub.

## Architecture

```text
USB-C UFP
  -> protection and 5 V rail
  -> USB2512B two-port USB 2.0 hub
       -> port 1: ESP32-S3-WROOM-1 native USB (Kivo controller)
       -> port 2: CM108B (USB audio input/output)

5 V -> 3.3 V regulator
       -> ESP32-S3, hub logic, display module, input pull-ups

ESP32-S3
  -> 3 x 6 diode-isolated key matrix
  -> SH1106 I2C + EC11 A/B/push + Confirm + Back
  -> three toggle-switch inputs
  -> hook-switch input

CM108B
  -> four-pin handset connector: microphone signal/ground and receiver/ground
```

The hub and audio codec are separate USB functions. The firmware never carries
audio samples, so keypad scanning and display updates cannot starve the audio
path.

## Board Composition

- Board: rectangular, approximately `180 mm x 105 mm`, four copper layers.
- Stackup intent: L1/L4 carry most signals, L2 is GND-dominant copper, and L3
  is 3V3-dominant copper. The P0 autorouter may cross the inner layers; pours
  clear those traces automatically rather than forming unbroken planes.
- Main interaction face: 18 MX-style switch footprints at `19.05 mm` pitch.
- Upper-left: three toggle switches and hook-state connector.
- Upper-center/right: 1x9 display-module connector in the exact signal order
  `CON/SDA/SCL/PSH/TRA/TRB/BAK/GND/3V3`.
- Upper edge: USB-C input and handset audio connector.
- Logic cluster: USB hub, ESP32-S3 module, CM108B, crystals, regulator,
  protection, and decoupling grouped above the key matrix.
- Four M3 mounting holes and a clear perimeter are included in the visual
  design.

The antenna end of the ESP32-S3 module faces a board edge and receives a
silkscreen keepout marker. USB differential pairs remain short inside the logic
cluster. The microphone path stays away from USB, the ESP32 antenna, and key
matrix scanning traces.

## Electrical Contracts

### Display Module J3

| Pin | Signal | Direction at main board | Treatment |
|---:|---|---|---|
| 1 | CON | input | active-low, 10 kOhm pull-up |
| 2 | SDA | bidirectional | 4.7 kOhm pull-up to 3.3 V |
| 3 | SCL | output | 4.7 kOhm pull-up to 3.3 V |
| 4 | PSH | input | active-low, 10 kOhm pull-up |
| 5 | TRA | input | EC11 phase A, 10 kOhm pull-up |
| 6 | TRB | input | EC11 phase B, 10 kOhm pull-up |
| 7 | BAK | input | active-low, 10 kOhm pull-up |
| 8 | GND | power | digital ground |
| 9 | 3V3 | power | regulated 3.3 V |

The screen is assumed to use I2C address `0x3C`; firmware may probe `0x3D` as a
fallback.

### Key Matrix

The keypad uses rows `ROW0..ROW2` and columns `COL0..COL5`. A 1N4148W diode is
placed in series with each switch, consistently oriented column-to-row. Exact
GPIO numbers remain a firmware mapping concern and are surfaced as named MCU
ports in the proof-of-concept.

### Persistent Inputs

`MODE1..MODE3` and `HOOK` use active-low contacts to ground with external
10 kOhm pull-ups. Firmware debounces all four signals and reports stable state,
not raw edges.

### USB And Power

- USB-C is a USB 2.0 device-only receptacle with 5.1 kOhm pull-downs on both CC
  pins.
- A resettable fuse and low-capacitance ESD protection precede the hub.
- The two downstream ports are permanently attached internal devices.
- The concept board is bus-powered and budgets less than 500 mA. Production
  work must measure ESP32-S3 radio peaks and audio-output load before retaining
  this assumption.
- A 3.3 V regulator supplies digital logic. Bulk and local decoupling are shown
  as functional groups rather than a manufacturer-validated layout.

### P0 Routing

- The four-layer board uses tscircuit's local autorouter for a complete visual
  routing pass across `top`, `inner1`, `inner2`, and `bottom`.
- `inner1` carries a GND copper pour and `inner2` carries a 3V3 copper pour.
  Six GND and four 3V3 through vias tie the outer copper to those planes from
  low-congestion edge corridors.
- Ordinary control, matrix, and low-current signals use `0.25 mm` copper with
  at least `0.20 mm` trace clearance.
- Primary `VBUS`, `VBUS_RAW`, `V3V3`, and ground-return branches use `0.50 mm`
  copper where they are represented by individual source traces.
- The upstream, ESP32-S3 downstream, and CM108B downstream USB D+/D- networks
  are all routed. The point-to-point ESP32-S3 and CM108B links use native
  differential-pair constraints with `0.20 mm` edge-to-edge gap and no more
  than `0.5 mm` routed-length skew. The upstream Type-C link remains a
  multi-terminal net because the receptacle exposes duplicate D+/D- pins and
  the ESD device is modeled as a shunt.
- Routing must produce at least one PCB trace for every routable source
  connection and no autorouting, missing-trace, clearance, or PCB trace errors.

These are P0 geometry constraints, not an impedance guarantee. A production
revision still requires an actual stackup, 90 ohm differential calculation,
continuous return-plane review, USB test coupons or measurement, and EMC
validation.

## Visual Language

The PCB uses black solder mask, exposed metal pads, and restrained white
silkscreen. Major zones are labelled `VOICE`, `CONTROL`, `MACROS`, and `USB`.
Key positions use identifiers `K01` through `K18`; no speculative user action
labels are baked into the hardware.

## Verification

Automated checks cover:

- the isolated hardware package installs and typechecks;
- the circuit renders without a fatal tscircuit error;
- the Circuit JSON contains 18 switches and 18 diodes;
- all nine display connector signals are present;
- the hub exposes exactly one upstream and two downstream USB pairs;
- the rendered PCB has a board outline and four mounting holes;
- a PCB SVG/PNG or viewer output can be generated for visual inspection.
- routed Circuit JSON contains copper traces and no routing errors;
- the rendered board reports four layers, inner GND/3V3 pours, and through-via
  connectivity for both plane nets;
- all three USB links and both valid point-to-point differential-pair
  constraints are present in the source model.

Manual review covers component overlap, readable silkscreen, keyboard pitch,
logical zone separation, and a nonblank PCB view.

## Explicit Limitations

- No Gerber release or fabrication claim is made in P0.
- Inner-layer pours are dominant power regions, not verified unbroken planes;
  the installed autorouter can also use those layers for signals.
- Exact footprints and 3D bodies are not guaranteed to match purchasable parts.
- USB impedance, return path, crystal loading, antenna keepout, audio gain,
  microphone bias, ESD performance, thermal behavior, and EMC are not validated.
- The handset receiver impedance and microphone type still require measurement.
- Firmware, host shortcut behavior, enclosure fit, and physical acceptance are
  outside this PCB-effect trial.

## Acceptance

P0 is complete when the tscircuit project runs locally, produces a coherent
schematic/PCB preview of the full workstation, passes structural checks, and
clearly identifies the remaining work before fabrication.
