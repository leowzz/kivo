# Kivo Integrated Workstation

This model combines an 18-key mechanical keypad, a display PCB, either
supported controller board, and a separately printed telephone handset base
into a compact desktop workstation.

All dimensions are millimeters.

## Printed Parts

- `kivo_integrated_workstation_shell.stl`: open-top `138 x 104` chassis,
  30-degree panel support lip, and six panel attachment holes.
- `kivo_integrated_workstation_sloped_panel.stl`: removable `132 x 117` key and
  display panel with six recessed fly-wire clips. It has no underside
  protrusions and remains support-free when printed flat.
- `kivo_integrated_workstation_bottom_cover.stl`: removable `138 x 104` bottom
  cover with a two-level snap-fit controller cradle and ventilation slots.
- `telephone_handset_switch_base_workstation_mount.stl`: independent handset
  base with two downward-opening T-slots. They slide over two upward T-rails on
  the chassis left wall without screws, a shared tray, or a bottom-cover
  extension.

The generator is `scripts/modeling/integrated_workstation.py`. Run it from the
repository root with:

```bash
uv run --script scripts/modeling/integrated_workstation.py
```

## Reference Dimensions

| Item | Designed interface |
|---|---:|
| Heat-set insert supplied | `3.9` body, `4.9` knurled rings, `4.9` long |
| Heat-set insert pocket | `4.0 x 5.4` blind bore, `5.1 x 0.6` lead-in |
| M3 countersunk screw | `2.9` thread, `5.3` measured head diameter |
| Printed countersink | `3.4` through hole, `5.6` diameter, `90 degrees` |
| Bottom cover | `138 x 104`, matching the controller chassis footprint |
| Bottom cover attachment | Four countersunk M3 screws |
| Handset side attachment | Two vertical T-slot hangers, no hardware |
| T-rail height | `13.6` |
| T-slot straight depth | `14.0` plus a support-free tapered roof |
| T-slot clearance | `0.3` around each rail surface |
| Key switches | `14.8` lower relief, `14.0` upper lip |
| Key layout | `6 x 3`, `19.05` pitch |
| Key plane | `30 degrees` above horizontal |
| Fly-wire retention | Six clips, one per three-key group |
| Wire clip pocket | `16.0 x 3.0 x 2.2`, with a `1.5` snap-in mouth |
| Sloped panel attachment | Six `3.4` clearances into hidden chassis inserts |
| Display PCB | `64.90 x 35.03`, four backside-loaded heat-set insert holes |
| Display attachment | Four `4.0` through bores with `5.1 x 0.6` backside lead-ins |
| Display header | 8 pins on PCB left half, first pin at `x=11.38`, `2.54` pitch |
| Display cable slot | `24.5 x 6.5`, aligned below the left-side header |
| Controller cradle | `28.64 x 63.89` maximum clear area |
| RP2040 reference | `22.86 x 53.34` |
| ESP32-S3 reference | `27.94 x 63.39` |
| RP2040 support level | PCB underside `3.0` above the cover inner face |
| ESP32-S3 support level | PCB underside `6.5` above the cover inner face |
| Controller USB opening | Rear wall, `37.0 x 9.5` |

The display PCB sits on the removable panel in a rounded locating bezel with
`0.65` clearance on each edge. The bezel rises only `2.0` above the 30-degree
key plane. Four M3 heat-set inserts are installed from the panel's flat back
side; their `4.0` bores pass through the full `5.4` mounting thickness, with a
`5.1 x 0.6` lead-in on the back. The dimensioned reference places its eight-pin
header on the left half of the board, so a hidden `24.5 x 6.5` slot is aligned
below that header rather than centered beneath the PCB.

The 18 switches are intended for direct fly-wiring rather than a hot-swap PCB.
Each key row has a left and right recessed wire clip, so every three switches
share one clip. Feed the thin wires into the `1.5` opening one at a time; the
opening expands behind two internal lips into a `3.0` pocket that retains the
bundle. The clips stop `1.2` short of the visible panel face.

## Hardware And Assembly

- 14 M3 heat-set brass inserts with the measured `3.9` body, `4.9` knurled
  rings, and `4.9` length: 6 for the sloped-panel attachment, 4 for the display,
  and 4 for the bottom cover. Chassis receiving parts use a `4.0 x 5.4` blind
  bore with a shallow `5.1` lead-in; the four display bores pass through the
  panel and open from its back side.
- The user-supplied screws measure `2.9` across the thread and `5.3` across
  the head. Exterior attachment holes use a `3.4` clearance bore and a
  `5.6`, 90-degree countersink so the screw heads can sit flush.
- 4 M3 x 8 screws for the bottom cover. They pass through the outside cover
  and enter inserts installed from the hidden underside of the chassis.
- 6 M3 x 10 screws for the removable sloped panel. They pass through the
  panel face into inserts installed in the chassis support bosses. The insert
  openings are covered completely by the assembled panel.
- 4 M3 x 10 screws for the display PCB.
- The RP2040 and ESP32-S3 reference boards have GPIO solder holes but no
  dedicated mechanical mounting holes. The controller cradle therefore uses
  two screw-free stepped support levels with flexible side catches.

1. Align the handset base's two downward-opening slots with the chassis's two
   upward rails. Insert the rails from the slot bottoms and slide the parts
   vertically until both bottoms align. Lift the handset base along the same
   path to remove it; no fasteners are used.
2. Place the controller USB end toward the wide rear connector opening and
   press the board into its matching snap level. The narrower RP2040 uses the
   inner lower level; the larger ESP32-S3 uses the outer upper level. Press the
   two side catches outward when removing a board.
3. Heat four inserts into the display holes from the panel's flat back side
   until flush. Install all 18 switches and fasten the display PCB from the
   front with four M3 x 10 screws. Fly-wire each three-key group and press its
   wires individually into the nearest recessed clip before attaching the
   panel to the chassis.
4. Heat six inserts into the chassis panel bosses from the sloped mating face.
   Place the completed panel on the support lip and drive six M3 x 10 screws
   through its face into the hidden inserts.
5. After testing the keypad and display, heat four inserts into the chassis
   bottom bosses and fasten the bottom cover from outside.
   Future key or display changes require reprinting only the panel.

## Printing

- Print the shell upright with its bottom rim on the build plate.
- Print the handset base upright in its modeled orientation. Both T-slot
  housings begin on the build plate, and each closed slot roof converges at less
  than 45 degrees. The chassis rails also rise directly from the build plate, so
  the complete side-hanger interface prints without support.
- Print the sloped panel separately with its large flat underside on the build
  plate. The switch steps and 2 mm screen bezel face upward and need no support.
  The six wire clips are recessed into that underside: their internal walls
  open at 45 degrees and their `3.0` pocket roofs are short bridges, so do not
  add support inside them. Use elephant-foot compensation if the `1.5` mouths
  print too tight.
- Print the bottom cover flat with the controller rails facing upward.
- The controller catches are thin vertical cantilevers. Print them in the
  modeled orientation and avoid adding support between a catch and the board
  pocket.
- Start with a 0.4 nozzle, 0.2 layer height, 4 perimeters, and 20 percent
  infill.
- The main shell and cover remain `138 mm` wide. The side-hanger rail extends
  the shell another `5.8 mm` to the left, for `143.8 mm` overall.
- The chassis has no broad sloped roof, so it does not need support beneath the
  key deck. The removable panel rests on two side rails, a front lip, and six
  insert bosses. Its rear center remains open, avoiding a long unsupported
  cross bridge at the top of the chassis.
- The rear pair of panel screws is moved 3 mm toward the front and the side
  rails stop at local `y=117`, keeping every panel support at or in front of
  the chassis rear wall (`y=108` in assembled coordinates).
- The removable panel's rear edge is shortened by 1 mm and the screen is moved
  forward by the same amount. In the assembled model its rear bound is
  `y=107.923`, so the panel also remains inside the `y=108` rear wall.

The generator rejects any result that is not a single positive, watertight,
consistently wound two-manifold mesh. It measures both levels of all 18 switch
apertures directly on the flat-print panel and verifies the six panel screw
paths, four bottom-cover screw paths, two handset T-slot interfaces, and assembly
clearance.
