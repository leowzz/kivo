# Kivo Integrated Workstation

This model combines the existing telephone-handset switch base, an 18-key
mechanical keypad, a display PCB, and either supported controller board into a
single desktop workstation.

All dimensions are millimeters.

## Printed Parts

- `kivo_integrated_workstation_shell.stl`: open-top chassis, handset-base
  pocket, 30-degree panel support lip, and six panel attachment holes.
- `kivo_integrated_workstation_sloped_panel.stl`: removable `132 x 117` key and
  display panel. Its underside is completely flat for support-free printing.
- `kivo_integrated_workstation_bottom_cover.stl`: removable full-width bottom
  cover with a rounded extension beneath the handset tray, two-level snap-fit
  controller cradle, and ventilation slots.
- `telephone_handset_switch_base_workstation_mount.stl`: the handset base with
  four aligned blind heat-set-insert holes for bottom-up attachment to the
  workstation.

The generator is `scripts/integrated_workstation.py`. Run it from the repository
root with:

```bash
uv run scripts/integrated_workstation.py
```

## Reference Dimensions

| Item | Designed interface |
|---|---:|
| Handset switch base | `63.8 x 78.8` body in a `65.0 x 80.0` pocket |
| Heat-set insert supplied | `3.9` body, `4.9` knurled rings, `4.9` long |
| Heat-set insert pocket | `4.0 x 5.4` blind bore, `5.1 x 0.6` lead-in |
| M3 countersunk screw | `2.9` thread, `5.3` measured head diameter |
| Printed countersink | `3.4` through hole, `5.6` diameter, `90 degrees` |
| Handset attachment | Four `3.4` tray clearance holes and four hidden insert pockets |
| Bottom cover | `210 x 104`, shaped to cover both the handset tray and controller chassis |
| Bottom cover attachment | Six countersunk M3 screws, including two beneath the handset tray |
| Key switches | `14.8` lower relief, `14.0` upper lip |
| Key layout | `6 x 3`, `19.05` pitch |
| Key plane | `30 degrees` above horizontal |
| Sloped panel attachment | Six `3.4` clearances into hidden chassis inserts |
| Display PCB | `64.90 x 35.03`, four M3 clearance holes |
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
key plane. Four M3 holes secure the PCB. The dimensioned reference places its
eight-pin header on the left half of the board, so a hidden `24.5 x 6.5` slot
is aligned below that header rather than centered beneath the PCB.

## Hardware And Assembly

- 16 M3 heat-set brass inserts with the measured `3.9` body, `4.9` knurled
  rings, and `4.9` length: 6 for the sloped panel, 4 for the handset base, and
  6 for the bottom cover. Each receiving part has a `4.0 x 5.4` blind bore and
  shallow `5.1` lead-in.
- The user-supplied screws measure `2.9` across the thread and `5.3` across
  the head. Exterior attachment holes use a `3.4` clearance bore and a
  `5.6`, 90-degree countersink so the screw heads can sit flush.
- 6 M3 x 8 screws for the bottom cover. They pass through the outside cover
  and enter inserts installed from the hidden underside of the chassis. Two
  of these fasteners secure the extension directly beneath the handset tray.
- 6 M3 x 10 screws for the removable sloped panel. They pass through the
  panel face into inserts installed in the chassis support bosses. The insert
  openings are covered completely by the assembled panel.
- 4 M3 x 8 screws for the handset base. Insert them from below through the
  shell's `3.4` clearance holes and into inserts installed from the hidden
  underside of the handset base.
- 4 M3 x 10 screws and nuts for the display PCB.
- The RP2040 and ESP32-S3 reference boards have GPIO solder holes but no
  dedicated mechanical mounting holes. The controller cradle therefore uses
  two screw-free stepped support levels with flexible side catches.

1. Place the controller USB end toward the wide rear connector opening and
   press the board into its matching snap level. The narrower RP2040 uses the
   inner lower level; the larger ESP32-S3 uses the outer upper level. Press the
   two side catches outward when removing a board.
2. Route the handset switch wires through the left-to-right internal cable
   passage.
3. Install all 18 switches and the display PCB on
   `kivo_integrated_workstation_sloped_panel.stl`, then complete its wiring on
   the flat panel before attaching it to the chassis.
4. Heat four inserts into the handset base's underside until flush, then place
   it into the left pocket. From the shell underside, drive four M3 x 8 screws
   upward through the tray and into the hidden inserts.
5. Heat six inserts into the chassis panel bosses from the sloped mating face.
   Place the completed panel on the support lip and drive six M3 x 10 screws
   through its face into the hidden inserts.
6. After testing the keypad, display, and handset switch, heat six inserts
   into the chassis bottom bosses and fasten the full-width bottom cover from
   outside. The cover closes the handset tray underside as well as the main
   controller chamber.
   Future key or display changes require reprinting only the panel.

## Printing

- Print the shell upright with its bottom rim on the build plate.
- Print the sloped panel separately with its large flat underside on the build
  plate. The switch steps and 2 mm screen bezel face upward and need no support.
- Print the bottom cover flat with the controller rails facing upward.
- The controller catches are thin vertical cantilevers. Print them in the
  modeled orientation and avoid adding support between a catch and the board
  pocket.
- Start with a 0.4 nozzle, 0.2 layer height, 4 perimeters, and 20 percent
  infill.
- The 210 mm shell width fits a nominal 220 mm bed but leaves little room for a
  skirt or brim. Check the slicer's printable area before starting.
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
paths and assembly clearance. It separately verifies that all four handset
screw paths are empty and aligned in the shell and modified handset base.
